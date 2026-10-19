//! The sparse-by-default working copy (C8, FR8.1, AC6.1).
//!
//! `checkout` pages `vcs.tree` for the changeset's blob listing — the object
//! store holds no tree objects, so selection runs client-side over blob
//! entries (AC6.1.1). Declared path filters select the subset; only the
//! selected entries' missing blobs are fetched through `object_get`
//! (want-fetch, AC3.4.1), each verified against its wanted hash before it
//! touches cache or disk (AC3.3.3/AC6.1.2). Materialization is
//! temp-write-then-rename per file (an interrupted run never leaves a
//! half-written file), preserving executable bits and symlinks on unix —
//! symlink creation degrades to a typed `CliFault::Fs` elsewhere (AC6.3.1).

use std::fs;
use std::io::Write;
use std::path::Path;

use oxidean_core::vcs_types::{VcsFileMode, VcsTreeResponse};
use oxidean_core::RpcResponse;
use serde_json::json;

use super::object_cache::ObjectCache;
use super::transport::{CliFault, VcsTransport};

/// Marker dropped in a checkout target dir — its presence distinguishes an
/// `ox vcs checkout`–owned dir from an arbitrary non-empty one (AC6.1.3).
pub const CHECKOUT_MARKER: &str = ".oxidean-vcs-checkout";

/// Tree page size — matches the server's 500-entry page bound.
const TREE_PAGE_LIMIT: i64 = 500;

/// Client-side path selection over the changeset's blob listing (AC3.4.1).
/// A declared filter matches an exact path or as a directory prefix
/// (`src`/`src/` both select `src/main.rs`). The empty set is the degenerate
/// full-checkout filter (AC3.4.2).
#[derive(Debug, Clone, Default)]
pub struct PathFilter {
    prefixes: Vec<String>,
}

impl PathFilter {
    /// `filters` are the declared `--paths` values; trailing `/` normalized.
    pub fn new(filters: &[String]) -> Self {
        let mut prefixes: Vec<String> = filters
            .iter()
            .map(|f| f.trim().trim_end_matches('/').to_string())
            .filter(|f| !f.is_empty())
            .collect();
        prefixes.sort();
        prefixes.dedup();
        Self { prefixes }
    }

    /// Empty filter set selects everything.
    pub fn is_full(&self) -> bool {
        self.prefixes.is_empty()
    }

    /// The empty set is the full-checkout filter (AC3.4.2) — matches all.
    pub fn matches(&self, path: &str) -> bool {
        self.prefixes.is_empty()
            || self
                .prefixes
                .iter()
                .any(|p| path == p || path.starts_with(&format!("{p}/")))
    }
}

/// Checkout tallies — the `CheckoutResult` the contract pins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CheckoutResult {
    pub files_written: u64,
    pub blobs_fetched: u64,
    pub cache_hits: u64,
    pub skipped_outside_filter: u64,
}

/// The non-empty-target check (AC6.1.3): a dir that exists, has entries, and
/// was not made by a prior `ox vcs checkout` (no marker) is refused — never
/// silently merged into.
fn check_target(target: &Path) -> Result<(), CliFault> {
    if !target.exists() {
        return Ok(());
    }
    if !target.is_dir() {
        return Err(CliFault::Fs(format!(
            "checkout target {} exists and is not a directory",
            target.display()
        )));
    }
    if target.join(CHECKOUT_MARKER).exists() {
        return Ok(());
    }
    let mut entries = fs::read_dir(target)
        .map_err(|e| CliFault::Fs(format!("reading {}: {e}", target.display())))?;
    if entries.next().is_some() {
        return Err(CliFault::Fs(format!(
            "checkout target {} is not empty",
            target.display()
        )));
    }
    Ok(())
}

/// Page `vcs.tree` for the full blob listing of `changeset_id`.
async fn fetch_tree(
    transport: &dyn VcsTransport,
    owner: &str,
    name: &str,
    changeset_id: &str,
) -> Result<Vec<oxidean_core::vcs_types::VcsTreeEntry>, CliFault> {
    let mut entries = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let input = json!({
            "owner": owner,
            "name": name,
            "changeset_id": changeset_id,
            "limit": TREE_PAGE_LIMIT,
            "cursor": cursor,
        });
        let envelope = transport.call("vcs.tree", input).await?;
        let data = match &envelope {
            RpcResponse::Ok { data, .. } => data.clone(),
            RpcResponse::Err { .. } => return Err(CliFault::Server(Box::new(envelope))),
        };
        let page: VcsTreeResponse = serde_json::from_value(data)
            .map_err(|e| CliFault::Transport(format!("decoding vcs.tree response: {e}")))?;
        entries.extend(page.entries);
        match page.cursor {
            Some(c) => cursor = Some(c),
            None => return Ok(entries),
        }
    }
}

/// Materialize one blob at `path` under `target` honoring `mode`.
fn materialize(target: &Path, rel: &str, mode: VcsFileMode, bytes: &[u8]) -> Result<(), CliFault> {
    // VCS paths are `/`-separated and server-validated; refuse anything that
    // would escape the target dir anyway (defense-in-depth for a hostile tree).
    if rel
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == "..")
    {
        return Err(CliFault::Fs(format!("unsafe path in tree entry: {rel}")));
    }
    let dest = target.join(rel);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| CliFault::Fs(format!("creating {}: {e}", parent.display())))?;
    }
    match mode {
        VcsFileMode::Symlink => {
            let link_target = String::from_utf8(bytes.to_vec())
                .map_err(|_| CliFault::Fs(format!("symlink target for {rel} is not utf-8")))?;
            symlink_replace(&link_target, &dest)?;
        }
        VcsFileMode::Regular | VcsFileMode::Executable => {
            write_atomic(&dest, bytes, mode)?;
        }
    }
    Ok(())
}

/// Temp-write-then-rename — per-file atomicity on interruption.
fn write_atomic(dest: &Path, bytes: &[u8], mode: VcsFileMode) -> Result<(), CliFault> {
    let dir = dest.parent().expect("dest has a parent dir");
    let tmp = dir.join(format!(
        ".oxidean-vcs-tmp-{}-{}",
        std::process::id(),
        dest.file_name().and_then(|f| f.to_str()).unwrap_or("blob")
    ));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        set_mode(&tmp, mode)?;
        fs::rename(&tmp, dest)
    })();
    result.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        CliFault::Fs(format!("writing {}: {e}", dest.display()))
    })
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: VcsFileMode) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let perm = match mode {
        VcsFileMode::Executable => fs::Permissions::from_mode(0o755),
        _ => fs::Permissions::from_mode(0o644),
    };
    fs::set_permissions(path, perm)
}

/// Non-unix filesystems carry no executable bit — degrade to a plain write.
#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: VcsFileMode) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn symlink_replace(link_target: &str, dest: &Path) -> Result<(), CliFault> {
    use std::os::unix::fs::symlink;
    if dest.symlink_metadata().is_ok() {
        fs::remove_file(dest)
            .map_err(|e| CliFault::Fs(format!("replacing {}: {e}", dest.display())))?;
    }
    symlink(link_target, dest)
        .map_err(|e| CliFault::Fs(format!("creating symlink {}: {e}", dest.display())))
}

/// Typed degradation (AC6.3.1): symlink materialization is unsupported here —
/// a clear fault, never a silent file copy.
#[cfg(not(unix))]
fn symlink_replace(_link_target: &str, dest: &Path) -> Result<(), CliFault> {
    Err(CliFault::Fs(format!(
        "symlink materialization unsupported on this platform: {}",
        dest.display()
    )))
}

/// Sparse working-copy materialization — see module docs (contract C8).
pub async fn checkout(
    transport: &dyn VcsTransport,
    owner: &str,
    name: &str,
    changeset_id: &str,
    filter: &PathFilter,
    target: &Path,
    cache: &mut ObjectCache,
) -> Result<CheckoutResult, CliFault> {
    check_target(target)?;
    fs::create_dir_all(target)
        .map_err(|e| CliFault::Fs(format!("creating {}: {e}", target.display())))?;
    let tree = fetch_tree(transport, owner, name, changeset_id).await?;
    let mut result = CheckoutResult::default();
    for entry in &tree {
        if !filter.matches(&entry.path) {
            result.skipped_outside_filter += 1;
            continue;
        }
        let hash = entry.blob.as_str();
        let bytes = match cache.get(hash)? {
            Some(bytes) => {
                result.cache_hits += 1;
                bytes
            }
            None => {
                let fetched = transport.object_get(owner, name, hash).await?;
                cache.put(hash, &fetched)?;
                result.blobs_fetched += 1;
                fetched
            }
        };
        materialize(target, &entry.path, entry.mode, &bytes)?;
        result.files_written += 1;
    }
    // Marker last: it only certifies a completed checkout.
    let marker = target.join(CHECKOUT_MARKER);
    fs::write(&marker, format!("{changeset_id}\n"))
        .map_err(|e| CliFault::Fs(format!("writing checkout marker {}: {e}", marker.display())))?;
    Ok(result)
}

/// Check whether `target` is a checkout-owned dir (marker present) and which
/// changeset it points at — informational for `ox vcs status --dir` flows.
pub fn checkout_marker(target: &Path) -> Option<String> {
    fs::read_to_string(target.join(CHECKOUT_MARKER))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::object_cache::{object_hash, ObjectCache};
    use crate::client::transport::TransportError;
    use serde_json::{json, Value};
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    /// Scripted transport: `vcs.tree` pops queued page payloads, `object_get`
    /// serves the blob map and records want-fetches.
    struct MockTransport {
        pages: Mutex<VecDeque<Value>>,
        objects: HashMap<String, Vec<u8>>,
        get_calls: Mutex<Vec<String>>,
        tree_inputs: Mutex<Vec<Value>>,
    }

    impl MockTransport {
        fn new(pages: Vec<Value>, objects: HashMap<String, Vec<u8>>) -> Self {
            Self {
                pages: Mutex::new(pages.into()),
                objects,
                get_calls: Mutex::new(Vec::new()),
                tree_inputs: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl VcsTransport for MockTransport {
        async fn call(&self, procedure: &str, input: Value) -> Result<RpcResponse, TransportError> {
            assert_eq!(procedure, "vcs.tree");
            if let Value::Array(_) = input {
                panic!("tree input must be an object");
            }
            let mut guard = self.tree_inputs.lock().unwrap();
            if let Some(page) = self.pages.lock().unwrap().pop_front() {
                guard.push(input);
                return Ok(RpcResponse::ok(page));
            }
            guard.push(input);
            Ok(RpcResponse::ok(json!({ "entries": [] })))
        }
        async fn object_get(
            &self,
            _owner: &str,
            _name: &str,
            hash: &str,
        ) -> Result<Vec<u8>, TransportError> {
            self.get_calls.lock().unwrap().push(hash.to_string());
            self.objects
                .get(hash)
                .cloned()
                .ok_or_else(|| TransportError::Http {
                    status: 404,
                    body: "vcs.not_found: no such object".into(),
                })
        }
        async fn object_put(
            &self,
            _owner: &str,
            _name: &str,
            _bytes: &[u8],
        ) -> Result<String, TransportError> {
            unimplemented!("checkout never puts objects")
        }
    }

    fn blob_entry(path: &str, mode: &str, content: &str) -> (Value, String) {
        let hash = object_hash(content.as_bytes());
        (json!({ "path": path, "mode": mode, "blob": hash }), hash)
    }

    fn single_page(entries: Vec<Value>) -> Value {
        json!({ "entries": entries })
    }

    #[test]
    fn path_filter_selection() {
        let full = PathFilter::new(&[]);
        assert!(full.is_full());
        assert!(full.matches("anything/at/all"));

        let f = PathFilter::new(&["src".to_string(), "docs/".to_string()]);
        assert!(!f.is_full());
        assert!(f.matches("src/main.rs"));
        assert!(f.matches("src")); // exact dir name selects
        assert!(f.matches("docs/guide.md")); // trailing `/` normalized
        assert!(!f.matches("srcx/nope"));
        assert!(!f.matches("other/file"));
    }

    #[tokio::test]
    async fn checkout_writes_files_marker_and_modes() {
        let (e1, h1) = blob_entry("src/main.rs", "100644", "fn main() {}");
        let (e2, h2) = blob_entry("bin/run.sh", "100755", "#!/bin/sh\n");
        let objects = HashMap::from([
            (h1, b"fn main() {}".to_vec()),
            (h2, b"#!/bin/sh\n".to_vec()),
        ]);
        let page = single_page(vec![e1, e2]);
        // Two identical pages — run 2 re-serves the same tree.
        let t = MockTransport::new(vec![page.clone(), page], objects);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wc");
        let mut cache = ObjectCache::new(dir.path().join("cache"), 1 << 20);

        let r = checkout(
            &t,
            "o",
            "n",
            "cs_1",
            &PathFilter::new(&[]),
            &target,
            &mut cache,
        )
        .await
        .unwrap();
        assert_eq!(r.files_written, 2);
        assert_eq!(r.blobs_fetched, 2);
        assert_eq!(r.cache_hits, 0);
        assert_eq!(
            fs::read(target.join("src/main.rs")).unwrap(),
            b"fn main() {}"
        );
        assert_eq!(checkout_marker(&target).as_deref(), Some("cs_1"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(target.join("bin/run.sh"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o755
            );
        }
        // A second checkout into the marked dir is allowed and cache-served.
        let r2 = checkout(
            &t,
            "o",
            "n",
            "cs_1",
            &PathFilter::new(&[]),
            &target,
            &mut cache,
        )
        .await
        .unwrap();
        assert_eq!(r2.cache_hits, 2);
        assert_eq!(
            t.get_calls.lock().unwrap().len(),
            2,
            "second run fetched nothing"
        );
    }

    #[tokio::test]
    async fn checkout_sparse_skips_and_counts() {
        let (e1, h1) = blob_entry("keep/a.txt", "100644", "a");
        let (e2, _) = blob_entry("drop/b.txt", "100644", "b");
        let objects = HashMap::from([(h1, b"a".to_vec())]);
        let t = MockTransport::new(vec![single_page(vec![e1, e2])], objects);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wc");
        let mut cache = ObjectCache::new(dir.path().join("cache"), 1 << 20);

        let filter = PathFilter::new(&["keep".to_string()]);
        let r = checkout(&t, "o", "n", "cs_1", &filter, &target, &mut cache)
            .await
            .unwrap();
        assert_eq!(r.files_written, 1);
        assert_eq!(r.skipped_outside_filter, 1);
        assert!(target.join("keep/a.txt").exists());
        assert!(!target.join("drop").exists());
    }

    #[tokio::test]
    async fn checkout_pages_tree_until_cursor_ends() {
        let (e1, h1) = blob_entry("a.txt", "100644", "a");
        let (e2, h2) = blob_entry("b.txt", "100644", "b");
        let objects = HashMap::from([(h1, b"a".to_vec()), (h2, b"b".to_vec())]);
        let t = MockTransport::new(
            vec![
                json!({ "entries": [e1], "cursor": "p2" }),
                single_page(vec![e2]),
            ],
            objects,
        );
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wc");
        let mut cache = ObjectCache::new(dir.path().join("cache"), 1 << 20);

        let r = checkout(
            &t,
            "o",
            "n",
            "cs_1",
            &PathFilter::new(&[]),
            &target,
            &mut cache,
        )
        .await
        .unwrap();
        assert_eq!(r.files_written, 2);
        let inputs = t.tree_inputs.lock().unwrap();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[1]["cursor"], json!("p2"));
    }

    #[tokio::test]
    async fn checkout_refuses_dirty_unmarked_dir() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("occupied");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("stray.txt"), b"x").unwrap();
        let t = MockTransport::new(vec![], HashMap::new());
        let mut cache = ObjectCache::new(dir.path().join("cache"), 1 << 20);
        let err = checkout(
            &t,
            "o",
            "n",
            "cs_1",
            &PathFilter::new(&[]),
            &target,
            &mut cache,
        )
        .await
        .expect_err("non-empty unmarked dir must be refused");
        assert!(err.to_string().contains("not empty"));

        // A marked dir is owned — checkout proceeds.
        fs::write(target.join(CHECKOUT_MARKER), "cs_0\n").unwrap();
        checkout(
            &t,
            "o",
            "n",
            "cs_1",
            &PathFilter::new(&[]),
            &target,
            &mut cache,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn checkout_tree_error_surfaces_as_server_fault() {
        struct ErrTransport;
        #[async_trait::async_trait]
        impl VcsTransport for ErrTransport {
            async fn call(&self, _p: &str, _i: Value) -> Result<RpcResponse, TransportError> {
                Ok(RpcResponse::err(oxidean_core::AppError::new(
                    "vcs.disabled",
                    "vcs is not enabled",
                )))
            }
            async fn object_get(
                &self,
                _: &str,
                _: &str,
                _: &str,
            ) -> Result<Vec<u8>, TransportError> {
                unreachable!()
            }
            async fn object_put(
                &self,
                _: &str,
                _: &str,
                _: &[u8],
            ) -> Result<String, TransportError> {
                unreachable!()
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wc");
        let mut cache = ObjectCache::new(dir.path().join("cache"), 1 << 20);
        let err = checkout(
            &ErrTransport,
            "o",
            "n",
            "cs_1",
            &PathFilter::new(&[]),
            &target,
            &mut cache,
        )
        .await
        .expect_err("server error must surface");
        assert_eq!(err.code(), "vcs.disabled", "envelope code rides verbatim");
    }

    #[tokio::test]
    async fn checkout_rejects_traversal_paths() {
        let (evil, h) = blob_entry("../escape.txt", "100644", "x");
        let objects = HashMap::from([(h, b"x".to_vec())]);
        let t = MockTransport::new(vec![single_page(vec![evil])], objects);
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wc");
        let mut cache = ObjectCache::new(dir.path().join("cache"), 1 << 20);
        let err = checkout(
            &t,
            "o",
            "n",
            "cs_1",
            &PathFilter::new(&[]),
            &target,
            &mut cache,
        )
        .await
        .expect_err("traversal must be refused");
        assert!(err.to_string().contains("unsafe path"));
        assert!(!dir.path().join("escape.txt").exists());
    }
}
