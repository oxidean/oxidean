//! The `OXIDEAN_STORAGE_*` / `OXIDEAN_S3_*` operator contract.
//!
//! `StorageConfig::from_env` parses once at boot; every malformed value is
//! a `Config` error (fail closed — a half-configured s3 deployment can
//! never silently fall back to fs semantics), matching the
//! `ObjectStoreConfig::from_env` precedent in `oxidean-vcs`.
//!
//! | Variable | Default | Notes |
//! |---|---|---|
//! | `OXIDEAN_STORAGE_BACKEND` | `fs` | `fs`/`s3`; anything else is fatal |
//! | `OXIDEAN_S3_BUCKET` | — | required when `s3` |
//! | `OXIDEAN_S3_PREFIX` | — | global key prefix before `{store}/` |
//! | `OXIDEAN_S3_ENDPOINT` | — | custom S3-compatible endpoint URL |
//! | `OXIDEAN_S3_REGION` | provider default | `auto` accepted (R2-style) |
//! | `OXIDEAN_S3_PATH_STYLE` | provider default | force path-style addressing |
//! | `OXIDEAN_S3_ACCESS_KEY_ID` | — | static key id; pair with secret |
//! | `OXIDEAN_S3_SECRET_ACCESS_KEY` | — | static secret; pair with key id |
//! | `OXIDEAN_S3_SESSION_TOKEN` | — | optional, static creds only |
//! | `OXIDEAN_S3_INSECURE_HTTP` | `0` | opt-in for `http://` endpoints |
//!
//! Static keys are both-or-neither — when neither is set the ambient AWS
//! chain applies (`AWS_*` env, web-identity/IRSA, ECS, IMDS), so Kubernetes
//! deployments authenticate without static secrets in env vars.
//! Credentials never appear in errors or logs.

use std::collections::HashMap;
use std::env::VarError;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use object_store::aws::{AmazonS3, AmazonS3Builder};

use crate::error::StorageError;
use crate::fs::{absolutize, FsBlobStore};
use crate::key::validate_store_name;
use crate::key::BlobKey;
use crate::s3::S3BlobStore;
use crate::store::BlobStore;

/// `fs` | `s3` selector env var.
pub const BACKEND_ENV: &str = "OXIDEAN_STORAGE_BACKEND";
/// Required for s3.
pub const S3_BUCKET_ENV: &str = "OXIDEAN_S3_BUCKET";
/// Optional global prefix prepended to every store (`{prefix}/{store}/`).
pub const S3_PREFIX_ENV: &str = "OXIDEAN_S3_PREFIX";
/// Optional custom endpoint for S3-compatible services.
pub const S3_ENDPOINT_ENV: &str = "OXIDEAN_S3_ENDPOINT";
/// Signing region; provider default when unset (`us-east-1` on AWS builder).
pub const S3_REGION_ENV: &str = "OXIDEAN_S3_REGION";
/// `1` forces path-style (`endpoint/bucket/key`) addressing.
pub const S3_PATH_STYLE_ENV: &str = "OXIDEAN_S3_PATH_STYLE";
/// Static access key id — must pair with `OXIDEAN_S3_SECRET_ACCESS_KEY`.
pub const S3_ACCESS_KEY_ID_ENV: &str = "OXIDEAN_S3_ACCESS_KEY_ID";
/// Static secret — never logged, never embedded in endpoints.
pub const S3_SECRET_ACCESS_KEY_ENV: &str = "OXIDEAN_S3_SECRET_ACCESS_KEY";
/// Optional session token for the static credentials.
pub const S3_SESSION_TOKEN_ENV: &str = "OXIDEAN_S3_SESSION_TOKEN";
/// `1` permits `http://` endpoints — required explicit opt-in.
pub const S3_INSECURE_HTTP_ENV: &str = "OXIDEAN_S3_INSECURE_HTTP";

/// Which [`BlobStore`] backend serves the non-git stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Local filesystem roots — today's behavior, the default.
    Fs,
    /// S3-compatible object storage.
    S3,
}

/// Boot-time storage selection — `from_env` validates everything before a
/// single byte moves.
#[derive(Debug)]
pub struct StorageConfig {
    /// Selected backend.
    pub backend: BackendKind,
    /// S3 parameters — present iff `backend == BackendKind::S3`.
    pub s3: Option<S3Config>,
}

impl StorageConfig {
    /// Parse `OXIDEAN_STORAGE_BACKEND` (+ `OXIDEAN_S3_*` when `s3`).
    ///
    /// - unset / `fs` → [`BackendKind::Fs`], no S3 config required.
    /// - `s3` → all `OXIDEAN_S3_*` validated; any invalid or incomplete
    ///   combination is a hard error.
    /// - anything else → `Config` (no silent fallback).
    pub fn from_env() -> Result<Self, StorageError> {
        match std::env::var(BACKEND_ENV) {
            Err(VarError::NotPresent) => Ok(Self {
                backend: BackendKind::Fs,
                s3: None,
            }),
            Err(e) => Err(e.into()),
            Ok(v) => match v.trim() {
                "fs" => Ok(Self {
                    backend: BackendKind::Fs,
                    s3: None,
                }),
                "s3" => Ok(Self {
                    backend: BackendKind::S3,
                    s3: Some(S3Config::from_env()?),
                }),
                other => Err(StorageError::Config(format!(
                    "{BACKEND_ENV}={other:?} is not supported (expected \"fs\" or \"s3\")"
                ))),
            },
        }
    }

    /// Build one store's [`BlobStore`] handle.
    ///
    /// `fs_root` is the store's existing `OXIDEAN_*_DIR`/`var/*` root in fs
    /// mode (marker-gated at open); in s3 mode the derived key prefix takes
    /// its place and `fs_root` is ignored. Both paths fail closed.
    pub async fn build(
        &self,
        store: &str,
        fs_root: &Path,
    ) -> Result<Arc<dyn BlobStore>, StorageError> {
        match self.backend {
            BackendKind::Fs => Ok(Arc::new(FsBlobStore::open(fs_root, store)?)),
            BackendKind::S3 => {
                let s3 = self.s3.as_ref().ok_or_else(|| {
                    StorageError::Config("s3 backend selected without S3Config".into())
                })?;
                Ok(Arc::new(S3BlobStore::connect(s3, store).await?))
            }
        }
    }

    /// Cross-store isolation — call once at boot with every named store's
    /// `(name, fs_root)` pair.
    ///
    /// - fs: refuses two stores resolving to the same root — the s3 mirror
    ///   of `ObjectStoreConfig`'s sibling-dir collision refusal.
    /// - s3: refuses two stores resolving to the same (bucket, prefix)
    ///   pair, which would mix ownership markers and make a store-level
    ///   sweep destroy a sibling's objects.
    pub fn check_store_isolation<'a, I>(&self, stores: I) -> Result<(), StorageError>
    where
        I: IntoIterator<Item = (&'a str, &'a Path)>,
    {
        match &self.s3 {
            Some(s3) => {
                let mut seen: HashMap<String, &str> = HashMap::new();
                for (store, _root) in stores {
                    let prefix = s3.store_prefix(store)?;
                    if let Some(other) = seen.insert(prefix.clone(), store) {
                        return Err(StorageError::Config(format!(
                            "stores `{other}` and `{store}` resolve to the same \
                             (bucket `{}`, prefix `{prefix}`)",
                            s3.bucket
                        )));
                    }
                }
            }
            None => {
                // Canonicalize so aliased roots (existing symlinks, dot
                // components) compare as the same identity; a root that
                // doesn't exist yet still gets lexical `.`/`..`
                // normalization via absolutize + components.
                let mut seen: HashMap<PathBuf, &str> = HashMap::new();
                for (store, root) in stores {
                    let root = std::fs::canonicalize(root).unwrap_or_else(|_| {
                        absolutize(root).components().collect()
                    });
                    if let Some((other_root, other)) =
                        seen.iter().find(|(other_root, _)| {
                            root.starts_with(other_root.as_path())
                                || other_root.as_path().starts_with(&root)
                        })
                    {
                        return Err(StorageError::Config(format!(
                            "stores `{other}` and `{store}` use overlapping \
                             roots `{}` and `{}`",
                            other_root.display(),
                            root.display()
                        )));
                    }
                    seen.insert(root, store);
                }
            }
        }
        Ok(())
    }
}

/// Where the static `OXIDEAN_S3_*` credentials came from — or that the
/// ambient AWS chain should resolve them at request time.
#[derive(Clone)]
pub enum S3Credentials {
    /// `OXIDEAN_S3_ACCESS_KEY_ID` + `OXIDEAN_S3_SECRET_ACCESS_KEY`
    /// (+ optional `OXIDEAN_S3_SESSION_TOKEN`).
    Static {
        /// The access key id.
        access_key_id: String,
        /// The secret access key — never Debug-printed.
        secret_access_key: String,
        /// Optional session token for the static pair.
        session_token: Option<String>,
    },
    /// Ambient chain: `AWS_*` env vars, profile, web-identity/IRSA, ECS
    /// task role, IMDS — resolved inside `AmazonS3Builder::from_env`.
    Ambient,
}

// Credentials must never reach logs — Debug redacts every key byte.
impl std::fmt::Debug for S3Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static { session_token, .. } => f
                .debug_struct("Static")
                .field("access_key_id", &"<redacted>")
                .field("secret_access_key", &"<redacted>")
                .field(
                    "session_token",
                    &session_token.as_ref().map(|_| "<redacted>"),
                )
                .finish(),
            Self::Ambient => f.write_str("Ambient"),
        }
    }
}

/// Parameters for the S3-compatible backend — parsed + validated by
/// [`S3Config::from_env`].
#[derive(Debug, Clone)]
pub struct S3Config {
    /// `OXIDEAN_S3_BUCKET` (required).
    pub bucket: String,
    /// `OXIDEAN_S3_PREFIX` — global prefix before `{store}/`.
    pub global_prefix: Option<String>,
    /// `OXIDEAN_S3_ENDPOINT` — custom endpoint for S3-compatible services.
    /// `None` selects the AWS endpoint constructed from `region`.
    pub endpoint: Option<String>,
    /// `OXIDEAN_S3_REGION` — `None` leaves the provider/builder default.
    pub region: Option<String>,
    /// `OXIDEAN_S3_PATH_STYLE` — `None` leaves the builder default
    /// (path-style when a custom endpoint is set, virtual-hosted on AWS).
    pub path_style: Option<bool>,
    /// `OXIDEAN_S3_INSECURE_HTTP` — explicit opt-in for `http://` endpoints.
    pub allow_http: bool,
    /// Static keys or the ambient chain.
    pub credentials: S3Credentials,
}

impl S3Config {
    /// Parse and validate the `OXIDEAN_S3_*` surface — every failure is
    /// [`StorageError::Config`], never a silent default.
    pub fn from_env() -> Result<Self, StorageError> {
        let bucket = env_required(S3_BUCKET_ENV)?;
        validate_bucket(&bucket)?;

        let global_prefix = match env_opt(S3_PREFIX_ENV)? {
            Some(prefix) => Some(validate_global_prefix(&prefix)?),
            None => None,
        };

        let allow_http = env_flag(S3_INSECURE_HTTP_ENV, false)?;
        let endpoint = match env_opt(S3_ENDPOINT_ENV)? {
            Some(ep) => Some(validate_endpoint(&ep, allow_http)?),
            None => None,
        };

        let region = match env_opt(S3_REGION_ENV)? {
            Some(r) => Some(validate_region(&r)?),
            None => None,
        };

        let path_style = env_bool_opt(S3_PATH_STYLE_ENV)?;

        let access_key_id = env_opt(S3_ACCESS_KEY_ID_ENV)?;
        let secret_access_key = env_opt(S3_SECRET_ACCESS_KEY_ENV)?;
        let session_token = env_opt(S3_SESSION_TOKEN_ENV)?;
        let credentials = match (access_key_id, secret_access_key, session_token) {
            (Some(access_key_id), Some(secret_access_key), session_token) => {
                S3Credentials::Static {
                    access_key_id,
                    secret_access_key,
                    session_token,
                }
            }
            (None, None, None) => S3Credentials::Ambient,
            (None, None, Some(_)) => {
                return Err(StorageError::Config(format!(
                    "{S3_SESSION_TOKEN_ENV} requires {S3_ACCESS_KEY_ID_ENV} and \
                     {S3_SECRET_ACCESS_KEY_ENV}"
                )))
            }
            _ => {
                return Err(StorageError::Config(format!(
                    "{S3_ACCESS_KEY_ID_ENV} and {S3_SECRET_ACCESS_KEY_ENV} must be \
                     set together (or neither, to use the ambient credential chain)"
                )))
            }
        };

        Ok(Self {
            bucket,
            global_prefix,
            endpoint,
            region,
            path_style,
            allow_http,
            credentials,
        })
    }

    /// `{global}/{store}` — or `{store}` with no global prefix. The store
    /// name is re-validated so a bad caller can never widen the key space.
    pub fn store_prefix(&self, store: &str) -> Result<String, StorageError> {
        validate_store_name(store)?;
        Ok(match &self.global_prefix {
            Some(global) => format!("{global}/{store}"),
            None => store.to_string(),
        })
    }

    /// Build the `AmazonS3` client. `from_env` runs first so ambient AWS_*
    /// config (region, endpoint, web-identity, container creds) applies;
    /// explicit `OXIDEAN_S3_*` values then win where set.
    pub fn build_client(&self) -> Result<AmazonS3, StorageError> {
        let mut builder = AmazonS3Builder::from_env()
            .with_bucket_name(self.bucket.clone())
            .with_allow_http(self.allow_http);
        if let Some(endpoint) = &self.endpoint {
            builder = builder.with_endpoint(endpoint.clone());
        }
        if let Some(region) = &self.region {
            builder = builder.with_region(region.clone());
        }
        if let Some(path_style) = self.path_style {
            builder = builder.with_virtual_hosted_style_request(!path_style);
        }
        match &self.credentials {
            S3Credentials::Static {
                access_key_id,
                secret_access_key,
                session_token,
            } => {
                builder = builder
                    .with_access_key_id(access_key_id.clone())
                    .with_secret_access_key(secret_access_key.clone());
                if let Some(token) = session_token {
                    builder = builder.with_token(token.clone());
                }
            }
            S3Credentials::Ambient => {}
        }
        builder
            .build()
            .map_err(|e| StorageError::Config(format!("s3 client build failed: {e}")))
    }
}

/// `Err(NotPresent)` → `None`; non-unicode → `Config`. Values are trimmed;
/// an empty-after-trim value counts as unset.
fn env_opt(name: &str) -> Result<Option<String>, StorageError> {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim().to_string();
            if v.is_empty() {
                Ok(None)
            } else {
                Ok(Some(v))
            }
        }
        Err(VarError::NotPresent) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn env_required(name: &str) -> Result<String, StorageError> {
    env_opt(name)?.ok_or_else(|| StorageError::Config(format!("{name} is required")))
}

/// Truthy {1,true,yes,on} / falsy {0,false,no,off} — anything else is a
/// `Config` error: a typo must never read as the default.
fn env_flag(name: &str, default: bool) -> Result<bool, StorageError> {
    match env_opt(name)? {
        None => Ok(default),
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => Err(StorageError::Config(format!(
                "{name}: not a boolean ({v:?})"
            ))),
        },
    }
}

fn env_bool_opt(name: &str) -> Result<Option<bool>, StorageError> {
    match env_opt(name)? {
        None => Ok(None),
        Some(v) => match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(Some(true)),
            "0" | "false" | "no" | "off" => Ok(Some(false)),
            _ => Err(StorageError::Config(format!(
                "{name}: not a boolean ({v:?})"
            ))),
        },
    }
}

/// S3 bucket naming: 3–63 of `[a-z0-9.-]`, alnum at both ends, no `..`,
/// not IPv4-shaped (which breaks virtual-hosted addressing).
fn validate_bucket(bucket: &str) -> Result<(), StorageError> {
    let ok = (3..=63).contains(&bucket.len())
        && bucket
            .bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.'))
        && bucket
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && bucket
            .bytes()
            .last()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && !bucket.contains("..")
        && !(bucket.bytes().all(|b| b.is_ascii_digit() || b == b'.') && bucket.contains('.'));
    if ok {
        Ok(())
    } else {
        Err(StorageError::Config(format!(
            "{S3_BUCKET_ENV}={bucket:?} is not a valid bucket name"
        )))
    }
}

/// The global prefix must normalize to valid key segments — a bad prefix
/// can never reach a key join.
fn validate_global_prefix(raw: &str) -> Result<String, StorageError> {
    let trimmed = raw.trim_matches('/');
    if trimmed.is_empty() {
        return Err(StorageError::Config(format!(
            "{S3_PREFIX_ENV} is not a usable prefix"
        )));
    }
    BlobKey::new(trimmed).map_err(|_| {
        StorageError::Config(format!("{S3_PREFIX_ENV}={raw:?} is not a valid key prefix"))
    })?;
    Ok(trimmed.to_string())
}

/// Endpoints are `http(s)://` only, carry no credentials or query, and
/// `http://` requires `OXIDEAN_S3_INSECURE_HTTP` — the `allow_http` opt-in
/// the `object_store` builder already enforces for AWS endpoints.
fn validate_endpoint(raw: &str, allow_http: bool) -> Result<String, StorageError> {
    let endpoint = raw.trim_end_matches('/');
    if !endpoint.starts_with("https://") && !endpoint.starts_with("http://") {
        return Err(StorageError::Config(format!(
            "{S3_ENDPOINT_ENV} must start with http:// or https://"
        )));
    }
    if endpoint.contains('@') {
        return Err(StorageError::Config(format!(
            "{S3_ENDPOINT_ENV} must not embed credentials"
        )));
    }
    if endpoint.contains('?') || endpoint.contains('#') {
        return Err(StorageError::Config(format!(
            "{S3_ENDPOINT_ENV} must not carry a query or fragment"
        )));
    }
    if endpoint.starts_with("http://") && !allow_http {
        return Err(StorageError::Config(format!(
            "{S3_ENDPOINT_ENV} uses http://; set {S3_INSECURE_HTTP_ENV}=1 to allow it"
        )));
    }
    Ok(endpoint.to_string())
}

/// Regions are `[A-Za-z0-9-]+` (`auto` is valid for R2-style providers).
fn validate_region(region: &str) -> Result<String, StorageError> {
    if region.is_empty()
        || !region
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err(StorageError::Config(format!(
            "{S3_REGION_ENV}={region:?} is not a valid region"
        )));
    }
    Ok(region.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Env-mutating tests must not race each other — nextest runs test fns
    /// on parallel threads in one process, so a single mutex serializes
    /// every env touch in this module.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Records pre-test env values once per name and restores them on drop.
    struct EnvGuard(Vec<(&'static str, Option<String>)>);

    impl EnvGuard {
        fn clear_all(names: &[&'static str]) -> Self {
            let mut guard = Self(Vec::new());
            for name in names {
                guard.clear(name);
            }
            guard
        }

        fn record(&mut self, name: &'static str) {
            if self.0.iter().all(|(n, _)| *n != name) {
                self.0.push((name, std::env::var(name).ok()));
            }
        }

        fn set(&mut self, name: &'static str, value: &str) {
            self.record(name);
            std::env::set_var(name, value);
        }

        fn clear(&mut self, name: &'static str) {
            self.record(name);
            std::env::remove_var(name);
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in self.0.drain(..) {
                match value {
                    Some(v) => std::env::set_var(name, v),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    const S3_VARS: &[&str] = &[
        S3_BUCKET_ENV,
        S3_PREFIX_ENV,
        S3_ENDPOINT_ENV,
        S3_REGION_ENV,
        S3_PATH_STYLE_ENV,
        S3_ACCESS_KEY_ID_ENV,
        S3_SECRET_ACCESS_KEY_ENV,
        S3_SESSION_TOKEN_ENV,
        S3_INSECURE_HTTP_ENV,
    ];

    fn s3_config() -> S3Config {
        S3Config {
            bucket: "bkt".into(),
            global_prefix: Some("oxidean".into()),
            endpoint: None,
            region: None,
            path_style: None,
            allow_http: false,
            credentials: S3Credentials::Ambient,
        }
    }

    /// All env-mutating assertions live in one fn — the lock serializes it
    /// anyway, but sequential checks in one test keep the guard state
    /// obvious (set → assert → reset).
    #[test]
    fn from_env_matrix() {
        let _lock = ENV_LOCK.lock().unwrap();
        let all: Vec<&'static str> = [BACKEND_ENV]
            .into_iter()
            .chain(S3_VARS.iter().copied())
            .collect();
        let mut g = EnvGuard::clear_all(&all);

        // Everything cleared → fs default.
        let cfg = StorageConfig::from_env().unwrap();
        assert_eq!(cfg.backend, BackendKind::Fs);
        assert!(cfg.s3.is_none());

        // Unknown backend refuses — no silent fallback.
        g.set(BACKEND_ENV, "minio");
        assert!(StorageConfig::from_env().is_err());

        // s3 without bucket refuses.
        g.set(BACKEND_ENV, "s3");
        let err = StorageConfig::from_env().unwrap_err();
        assert!(matches!(err, StorageError::Config(_)), "{err:?}");

        // Minimal s3 parses; credentials default to the ambient chain.
        g.set(S3_BUCKET_ENV, "my-bucket");
        let cfg = StorageConfig::from_env().unwrap();
        assert_eq!(cfg.backend, BackendKind::S3);
        let s3 = cfg.s3.as_ref().unwrap();
        assert_eq!(s3.bucket, "my-bucket");
        assert!(matches!(&s3.credentials, S3Credentials::Ambient));

        // Half-set static credentials refuse; the pair parses.
        g.set(S3_ACCESS_KEY_ID_ENV, "AKID");
        assert!(StorageConfig::from_env().is_err());
        g.set(S3_SECRET_ACCESS_KEY_ENV, "sekret");
        assert!(StorageConfig::from_env().is_ok());
        g.clear(S3_ACCESS_KEY_ID_ENV);
        g.clear(S3_SECRET_ACCESS_KEY_ENV);

        // Session token without static keys refuses.
        g.set(S3_SESSION_TOKEN_ENV, "tok");
        assert!(StorageConfig::from_env().is_err());
        g.clear(S3_SESSION_TOKEN_ENV);

        // http endpoint needs the explicit opt-in; https does not.
        g.set(S3_ENDPOINT_ENV, "http://minio:9000");
        assert!(StorageConfig::from_env().is_err());
        g.set(S3_INSECURE_HTTP_ENV, "1");
        assert!(StorageConfig::from_env().is_ok());
        g.clear(S3_INSECURE_HTTP_ENV);
        g.set(S3_ENDPOINT_ENV, "https://minio:9000");
        assert!(StorageConfig::from_env().is_ok());
        g.clear(S3_ENDPOINT_ENV);

        // Credentials-in-URL and query-carrying endpoints refuse.
        g.set(S3_ENDPOINT_ENV, "https://user:pw@minio:9000");
        assert!(StorageConfig::from_env().is_err());
        g.set(S3_ENDPOINT_ENV, "https://minio:9000/?x=1");
        assert!(StorageConfig::from_env().is_err());
        g.clear(S3_ENDPOINT_ENV);

        // Bad booleans refuse rather than silently default.
        g.set(S3_PATH_STYLE_ENV, "maybe");
        assert!(StorageConfig::from_env().is_err());
        g.set(S3_PATH_STYLE_ENV, "0");
        assert!(StorageConfig::from_env().is_ok());
        g.clear(S3_PATH_STYLE_ENV);

        // Bad bucket / prefix / region refuse.
        g.set(S3_BUCKET_ENV, "Bad_Bucket");
        assert!(StorageConfig::from_env().is_err());
        g.set(S3_BUCKET_ENV, "good-bucket");
        g.set(S3_PREFIX_ENV, "a/../b");
        assert!(StorageConfig::from_env().is_err());
        g.set(S3_PREFIX_ENV, "/oxidean/");
        assert!(StorageConfig::from_env().is_ok());
        g.set(S3_REGION_ENV, "not a region!");
        assert!(StorageConfig::from_env().is_err());
    }

    #[test]
    fn prefix_derivation_and_isolation() {
        // No global prefix → prefix is the store name.
        let mut cfg = s3_config();
        cfg.global_prefix = None;
        assert_eq!(cfg.store_prefix("lfs").unwrap(), "lfs");
        cfg.global_prefix = Some("oxidean".into());
        assert_eq!(cfg.store_prefix("lfs").unwrap(), "oxidean/lfs");
        assert!(cfg.store_prefix("Bad Name").is_err());

        // Global prefix normalization.
        assert_eq!(validate_global_prefix("/oxidean/").unwrap(), "oxidean");
        assert!(validate_global_prefix("a/../b").is_err());
        assert!(validate_global_prefix("///").is_err());

        // s3 isolation: colliding (bucket, prefix) refuses.
        let storage = StorageConfig {
            backend: BackendKind::S3,
            s3: Some(cfg),
        };
        let root = Path::new("/ignored");
        storage
            .check_store_isolation([("lfs", root), ("packages", root)])
            .unwrap();
        assert!(storage
            .check_store_isolation([("lfs", root), ("lfs", root)])
            .is_err());

        // fs isolation: same resolved root refuses.
        let storage_fs = StorageConfig {
            backend: BackendKind::Fs,
            s3: None,
        };
        storage_fs
            .check_store_isolation([
                ("lfs", Path::new("/var/lfs")),
                ("packages", Path::new("/var/packages")),
            ])
            .unwrap();
        assert!(storage_fs
            .check_store_isolation([
                ("lfs", Path::new("/var/same")),
                ("packages", Path::new("/var/same")),
            ])
            .is_err());
    }

    #[test]
    fn credentials_debug_redacts_secrets() {
        let creds = S3Credentials::Static {
            access_key_id: "AKIDEXAMPLE".into(),
            secret_access_key: "supersecret".into(),
            session_token: Some("rawsessiontoken".into()),
        };
        let dbg = format!("{creds:?}");
        assert!(!dbg.contains("AKIDEXAMPLE"));
        assert!(!dbg.contains("supersecret"));
        assert!(!dbg.contains("rawsessiontoken"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn bucket_validation() {
        for ok in ["my-bucket", "a23", "a.b.c", "garage-1"] {
            assert!(validate_bucket(ok).is_ok(), "{ok}");
        }
        for bad in [
            "ab",
            "-abc",
            "abc-",
            "a..b",
            "1.2.3.4",
            "A_B",
            &"a".repeat(64),
        ] {
            assert!(validate_bucket(bad).is_err(), "{bad}");
        }
    }
}
