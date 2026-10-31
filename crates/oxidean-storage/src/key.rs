//! [`BlobKey`] — the only key shape allowed into a store. Keys derive from
//! already-validated identifiers (`validate_oid` hashes, `storage_rel`
//! paths, DB ids) — never raw client input — so the rules here are the last
//! mechanical gate before a path or prefix join.

use std::fmt;

use crate::error::StorageError;

const MAX_KEY_LEN: usize = 1024;

/// A validated store-relative object key: `/`-separated segments of
/// `[A-Za-z0-9._~-]`, never absolute, never a `.`/`..` component.
///
/// Covers every existing layout — LFS `{aa}/{bb}/{oid}`, template
/// `sha256/{aa}/{bb}/{digest}.zip`, artifacts `{run_id}/{id}.tgz`,
/// cache `{repo_id}/{id}.tgz`, pages site trees.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobKey(String);

impl BlobKey {
    /// Validate `raw` as a store-relative key. Anything rejected here can
    /// never reach a filesystem path or object prefix.
    pub fn new(raw: &str) -> Result<Self, StorageError> {
        validate_blob_key(raw)?;
        Ok(Self(raw.to_string()))
    }

    /// The key in `a/b/c` form — safe for `Path::join` and object prefixes.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BlobKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for BlobKey {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for BlobKey {
    type Error = StorageError;

    fn try_from(raw: &str) -> Result<Self, Self::Error> {
        Self::new(raw)
    }
}

/// Shared segment validator — the same rule guards `OXIDEAN_S3_PREFIX`
/// (each segment must be a valid key component).
pub(crate) fn validate_blob_key(raw: &str) -> Result<(), StorageError> {
    if raw.is_empty() || raw.len() > MAX_KEY_LEN {
        return Err(StorageError::InvalidKey(
            "empty or longer than 1024 bytes".into(),
        ));
    }
    if raw.starts_with('/') {
        return Err(StorageError::InvalidKey("absolute key".into()));
    }
    for segment in raw.split('/') {
        if segment.is_empty() {
            return Err(StorageError::InvalidKey("empty segment".into()));
        }
        if segment == "." || segment == ".." {
            return Err(StorageError::InvalidKey("dot segment".into()));
        }
        if !segment.bytes().all(valid_segment_byte) {
            return Err(StorageError::InvalidKey(format!(
                "segment {segment:?} holds an unsupported byte"
            )));
        }
    }
    Ok(())
}

fn valid_segment_byte(b: u8) -> bool {
    matches!(b, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'_' | b'~' | b'-')
}

/// Store names feed marker files and key prefixes — restrict to
/// `[a-z0-9][a-z0-9-]*` so `.oxidean-{store}-marker` and `{store}/` joins
/// can never smuggle separators or traversal.
pub fn validate_store_name(name: &str) -> Result<(), StorageError> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name.bytes().enumerate().all(|(i, b)| {
            if i == 0 {
                b.is_ascii_lowercase() || b.is_ascii_digit()
            } else {
                b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
            }
        });
    if ok {
        Ok(())
    } else {
        Err(StorageError::InvalidStoreName(format!(
            "{name:?} (expected [a-z0-9][a-z0-9-]*)"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_accept_real_store_layouts() {
        for ok in [
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "ab/cd/e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "sha256/ab/cd/deadbeef.zip",
            "run_42/artifact-17.tgz",
            "repo-9/cache-7.tgz",
            "pages/site-1/index.html",
            "u/a-b_c~d.e",
        ] {
            assert!(BlobKey::new(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn keys_reject_traversal_and_bad_bytes() {
        for bad in [
            "", "/abs", "a//b", "a/./b", "../up", "a/../b", "a\\b", "a b", "a?b", "a#b", "a*b",
            "a:b", "é", "a\0b",
        ] {
            assert!(BlobKey::new(bad).is_err(), "{bad:?}");
        }
        let too_long = "a".repeat(1025);
        assert!(BlobKey::new(&too_long).is_err());
    }

    #[test]
    fn store_names() {
        for ok in [
            "lfs",
            "packages",
            "release-assets",
            "vcs-objects",
            "a",
            "9x",
        ] {
            assert!(validate_store_name(ok).is_ok(), "{ok}");
        }
        for bad in ["", "-lfs", "Lfs", "lf s", "lf_s", "a/b", ".."] {
            assert!(validate_store_name(bad).is_err(), "{bad:?}");
        }
    }
}
