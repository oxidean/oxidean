//! Three-way merge + first-parent rebase engine (u5) — synchronous, total,
//! write-through.
//!
//! `MergeEngine::merge` / `MergeEngine::rebase` read trees through
//! `ReadIndex` (`merge_base` supplies the base TREE only — the pick may be
//! dominated, so ancestry decisions run the engine's own bounded BFS),
//! classify every path of `base ∪ ours ∪ theirs`, and write merged blobs
//! through `ObjectStore::put` + the changeset through `ChangesetStore::create`
//! — never `vcs_refs` (BR5.12).
//!
//! Two-pass write discipline (BR5.3/SD5.3): `put` repo-links unconditionally
//! and linked objects are never GC-eligible, so pass 1 runs the whole
//! classify+merge3 computation recording only content-derived sha256 mints —
//! zero `put`s. The conflict checkpoint, `manifest_diff` + its manifest cap,
//! and the mirrored create-input bounds all land BEFORE pass 2 re-reads the
//! inputs, re-runs `merge3`, `put`s (asserting `PutOutcome.hash` equals the
//! pass-1 mint), and `create`s.
//!
//! `merge3` runs inside `tokio::task::spawn_blocking` behind `run_merge3`'s
//! `JoinHandle` seam (BR5.10); the line/terminator model is byte-faithful:
//! `ends_nl` is caller-derived and last-line sameness compares
//! `(line_bytes, ends_nl)` pairs.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::convert::Infallible;
use std::ops::Range;

use bytes::Bytes;
use futures_util::stream;
use oxidean_core::vcs_types::{ChangesetId, VcsConflictEntry};
use oxidean_db::{Database, VcsChangesetRow};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tokio::task::JoinHandle;

use crate::changeset_store::{
    ChangeKind, ChangesetError, ChangesetStore, CreateChangesetInput, FileChangeInput,
};
use crate::object_store::{ObjectStore, ObjectStoreError, QuotaScope};
use crate::read_index::{split_lines, IndexError, ReadIndex, TreeEntry};
use crate::text_diff::{diff_lines, LineHunk, LINE_CAP};

/// `tree()` page size — u4's `SCAN_PAGE` stays private; same magnitude.
const MERGE_PAGE: usize = 500;
/// `is_text` probes the first 8 KiB for a NUL byte (BR5.5).
const TEXT_PROBE_BYTES: usize = 8 * 1024;
/// `VcsConflictEntry.detail` ceiling in CHARS (SR5.6/BR5.8).
const DETAIL_MAX_CHARS: usize = 200;
/// Mirrored from `changeset_store` — `create` re-validates authoritatively.
const MAX_AUTHOR_CHARS: usize = 255;
const MAX_MESSAGE_BYTES: usize = 65_535;
/// Non-regular modes never diff3 — pick rules only (BR5.6).
const MODE_SYMLINK: i64 = 0o120000;
const MODE_TREE: i64 = 0o040000;

/// Merge/rebase failure surface. u7 maps variants to wire codes (WF6).
/// Deliberately no `PartialEq` — `VcsConflictEntry` lacks it (recorded
/// deviation; tests field-compare).
#[derive(Debug)]
pub enum MergeError {
    /// A head/parent changeset does not exist.
    NotFound,
    /// Fail-closed input rejection — `field: reason` text, never echoes the
    /// rejected value (`ChangesetId::parse`'s text embeds it — discarded).
    InvalidInput(String),
    /// `merge`/`merge_base` on disjoint histories.
    NoCommonAncestor,
    /// A named cap tripped (tree/manifest/blob/walk counts) — honest bound,
    /// never truncation.
    TooLarge,
    /// `ObjectStore::put` quota denial — `QuotaScope` preserved for u7.
    QuotaExceeded(QuotaScope),
    /// Conflict entries, path-sorted; `detail` is path text only, ≤200 chars.
    Conflict(Vec<VcsConflictEntry>),
    /// `Database::is_skipped()` — eager fail-closed check (u4 convention).
    DbNotConfigured,
    /// Raw facade/io/store detail for host diagnostics — opaque to the wire.
    Internal(String),
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MergeError::NotFound => write!(f, "not found"),
            MergeError::InvalidInput(reason) => write!(f, "invalid input: {reason}"),
            MergeError::NoCommonAncestor => write!(f, "no common ancestor"),
            MergeError::TooLarge => write!(f, "too large"),
            MergeError::QuotaExceeded(scope) => {
                write!(f, "{}", ObjectStoreError::QuotaExceeded(*scope))
            }
            // Count only — `warned` logs `%e`; listing entries would leak
            // paths (devsecops 3).
            MergeError::Conflict(entries) => write!(f, "conflict: {} paths", entries.len()),
            MergeError::DbNotConfigured => write!(f, "database not configured"),
            MergeError::Internal(detail) => write!(f, "internal error: {detail}"),
        }
    }
}

impl std::error::Error for MergeError {}

/// Every typed-error return warns once at construction — `error = %e` carries
/// the typed error only; never path/message/author/blob values (SD5.8).
fn warned(e: MergeError) -> MergeError {
    tracing::warn!(error = %e, "vcs merge typed error");
    e
}

fn internal(detail: String) -> MergeError {
    warned(MergeError::Internal(detail))
}

fn invalid(field: &str, reason: &str) -> MergeError {
    warned(MergeError::InvalidInput(format!("{field}: {reason}")))
}

fn too_large() -> MergeError {
    warned(MergeError::TooLarge)
}

/// The complete Q8 error bridge.
fn map_index_err(e: IndexError) -> MergeError {
    match e {
        IndexError::BaseNotFound => warned(MergeError::NoCommonAncestor),
        IndexError::NotFound | IndexError::PathNotFound => warned(MergeError::NotFound),
        IndexError::FrontierTooLarge => too_large(),
        // Engine-minted cursors — malformed means index corruption.
        IndexError::CursorMalformed => internal(e.to_string()),
        IndexError::InvalidInput(reason) => warned(MergeError::InvalidInput(reason)),
        IndexError::DbNotConfigured => warned(MergeError::DbNotConfigured),
        IndexError::Internal(detail) => internal(detail),
    }
}

fn map_changeset_err(e: ChangesetError) -> MergeError {
    match e {
        // Raced head/parent deletes degrade to not-found.
        ChangesetError::ParentNotFound | ChangesetError::ChangesetNotFound => {
            warned(MergeError::NotFound)
        }
        // A stored manifest's blob is gone — integrity loss.
        ChangesetError::ObjectNotFound { .. } => internal(e.to_string()),
        ChangesetError::InvalidInput(reason) => warned(MergeError::InvalidInput(reason)),
        ChangesetError::DbNotConfigured => warned(MergeError::DbNotConfigured),
        ChangesetError::Internal(detail) => internal(detail),
    }
}

fn map_object_err(e: ObjectStoreError) -> MergeError {
    match e {
        ObjectStoreError::QuotaExceeded(scope) => warned(MergeError::QuotaExceeded(scope)),
        ObjectStoreError::DbNotConfigured => warned(MergeError::DbNotConfigured),
        // NotFound (raced delete mid-read), InvalidHash, Stream, Config, Db,
        // Io, Internal — defensive-Internal.
        other => internal(other.to_string()),
    }
}

/// Engine-tunable bounds (SR5.4) — constructor-injected so tests trip them
/// with small fixtures.
#[derive(Debug, Clone)]
pub struct MergeCaps {
    /// Per-blob read cap AND merged-output cap (bytes).
    pub merge_blob_max: u64,
    /// Bounds EACH accumulated whole-tree AND the emitted manifest.
    pub merge_paths_max: usize,
    /// `is_ancestor` BFS + the `side_manifest_pair` ancestry mark phase.
    pub ancestor_probe_max: usize,
    /// Each side-exclusive manifest walk.
    pub side_walk_max: usize,
    /// Collected `rename` rows across one whole side walk.
    pub rename_probe_max: usize,
    /// Conflict entries collected is `merge_conflict_max` + 1 — the +1 drives
    /// the wire `truncated` flag (LC5.8 supersedure).
    pub merge_conflict_max: usize,
}

impl Default for MergeCaps {
    fn default() -> Self {
        Self {
            merge_blob_max: 4 * 1024 * 1024,
            merge_paths_max: 10_000,
            ancestor_probe_max: 250_000,
            side_walk_max: 25_000,
            rename_probe_max: 10_000,
            merge_conflict_max: 500,
        }
    }
}

impl MergeCaps {
    /// Construction-time pin (LC5.2): `merge_blob_max` past
    /// `max_object_bytes` would trip `put`'s streamed cap deterministically
    /// mid-pass-2; `u64::MAX` would wrap `take(max + 1)`; `merge_paths_max`
    /// above the wire's 10k manifest bound could persist a manifest no
    /// client could submit.
    fn validate(&self, objects: &ObjectStore) -> Result<(), MergeError> {
        if self.merge_blob_max > objects.config.max_object_bytes {
            return Err(invalid(
                "merge_blob_max",
                "exceeds object store max_object_bytes",
            ));
        }
        if self.merge_blob_max == u64::MAX {
            return Err(invalid("merge_blob_max", "must be below u64::MAX"));
        }
        if self.merge_paths_max > 10_000 {
            return Err(invalid(
                "merge_paths_max",
                "exceeds the wire manifest bound",
            ));
        }
        Ok(())
    }
}

/// `merge`/`rebase` result — `FastForward` covers both the merge fast path
/// and the rebase no-op (`head` is the pre-existing id; zero rows written).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// An ancestor/descendant pair — the descendant wins; nothing written.
    FastForward { head: ChangesetId },
    /// A new changeset was created (or idempotency-resolved).
    Merged { changeset_id: ChangesetId },
}

/// The pinned conflict taxonomy (AC2.4.1/2.4.2) — open string on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    /// merge3 unstable regions.
    Content,
    /// A NUL-in-8KiB side diverged.
    Binary,
    /// Exec-bit divergence (BR5.24 conservative — no auto-pick).
    Mode,
    /// File ↔ symlink/tree divergence (BR5.6 — never diff3'd).
    Type,
    /// Cross-path provenance collisions (rename/rename, rename/delete).
    Rename,
    /// Delete-vs-modify — stands alone.
    Delete,
}

impl ConflictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConflictKind::Content => "content",
            ConflictKind::Binary => "binary",
            ConflictKind::Mode => "mode",
            ConflictKind::Type => "type",
            ConflictKind::Rename => "rename",
            ConflictKind::Delete => "delete",
        }
    }
}

/// One both-diverged region — side-coordinate line ranges, feeds `detail`
/// counts only (internal detail of `Merge3Fail`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRegion {
    pub ours_range: Range<usize>,
    pub theirs_range: Range<usize>,
}

/// `merge3`'s inner error — the cap must be representable inside
/// `spawn_blocking` (BR5.22); the caller maps variants (internal detail).
#[derive(Debug)]
pub enum Merge3Fail {
    /// `(n_base + n_side + 2) > LINE_CAP` newline byte-count pre-check.
    TooLarge,
    /// Both-diverged unstable regions — surfaced as a `content` conflict.
    Regions(Vec<ConflictRegion>),
}

/// Whole-tree accumulation `path → entry` (mid-walk `merge_paths_max` abort).
type TreeMap = BTreeMap<String, TreeEntry>;

/// `old_path → final tip-visible new_path` provenance for one side.
type SideManifest = BTreeMap<String, String>;

/// Per-path classify outcome — `Resolved(None)` is a clean deletion.
enum Classified {
    Resolved(Option<TreeEntry>),
    Conflict(VcsConflictEntry),
}

fn entry(path: &str, kind: ConflictKind, detail: Option<String>) -> VcsConflictEntry {
    VcsConflictEntry {
        path: path.to_string(),
        kind: kind.as_str().to_string(),
        detail,
    }
}

/// `detail` ≤200 chars on a char boundary (BR5.8; silent-by-design).
fn detail200(s: &str) -> String {
    s.chars().take(DETAIL_MAX_CHARS).collect()
}

/// Collect `merge_conflict_max` + 1 — the extra entry drives `truncated`.
fn push_conflict(v: &mut Vec<VcsConflictEntry>, cap: usize, e: VcsConflictEntry) {
    if v.len() <= cap {
        v.push(e);
    }
}

fn ent_eq(a: Option<&TreeEntry>, b: Option<&TreeEntry>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.mode == y.mode && x.blob_hash == y.blob_hash,
        _ => false,
    }
}

fn is_nonregular(mode: i64) -> bool {
    mode == MODE_SYMLINK || mode == MODE_TREE
}

/// Fixed echo-free parse error — `ChangesetId::parse` embeds the raw id in
/// its own text (devsecops 8).
fn parse_changeset_id(raw: &str) -> Result<ChangesetId, MergeError> {
    ChangesetId::parse(raw).map_err(|_| {
        warned(MergeError::InvalidInput(
            "changeset id: malformed".to_string(),
        ))
    })
}

/// Mirrored `compose_author`/`message` bounds (`changeset_store.rs:439-461`
/// exactly — trimmed-name nonempty, email nonempty un-trimmed, no `<`/`>`/NL/
/// CR/control bytes, composed ≤255 CHARS, message ≤65_535 BYTES incl. the
/// synthesized default). Runs at entry AND after the manifest checkpoint so
/// no caller `InvalidInput` can fire post-`put` (BR5.16 dual validation).
fn check_inputs(author_name: &str, author_email: &str, message: &str) -> Result<(), MergeError> {
    let name = author_name.trim();
    if name.is_empty() || author_email.is_empty() {
        return Err(invalid("author", "name and email are required"));
    }
    for part in [name, author_email] {
        for b in part.bytes() {
            if b == b'<' || b == b'>' || b == b'\n' || b == b'\r' || b < 0x20 || b == 0x7f {
                return Err(invalid(
                    "author",
                    "contains '<', '>', newline, or control byte",
                ));
            }
        }
    }
    let composed = format!("{name} <{author_email}>");
    if composed.chars().count() > MAX_AUTHOR_CHARS {
        return Err(invalid("author", "exceeds 255 characters"));
    }
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(invalid("message", "exceeds 65535 bytes"));
    }
    Ok(())
}

/// sha256 hex of `bytes` — the pass-1 mint; pass-2 asserts `PutOutcome.hash`
/// equals it (`put` mints the identical recipe).
fn local_blob_id(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// diff3-eligible iff no `0x00` in the first 8 KiB (BR5.5) — checked only
/// after the size pre-step (BR5.21); non-regular modes never reach it.
fn is_text(bytes: &[u8]) -> bool {
    !bytes.iter().take(TEXT_PROBE_BYTES).any(|b| *b == 0)
}

/// The merge/rebase engine — a synchronous, total computation over u4 reads
/// with writes only through `ChangesetStore::create` + `ObjectStore::put`.
pub struct MergeEngine {
    db: Database,
    index: ReadIndex,
    store: ChangesetStore,
    objects: ObjectStore,
    caps: MergeCaps,
}

impl MergeEngine {
    /// Validates `caps` against the object store's byte ceiling (LC5.2) —
    /// host misconfig surfaces as `InvalidInput`.
    pub fn new(
        db: Database,
        index: ReadIndex,
        store: ChangesetStore,
        objects: ObjectStore,
        caps: MergeCaps,
    ) -> Result<Self, MergeError> {
        caps.validate(&objects)?;
        Ok(Self {
            db,
            index,
            store,
            objects,
            caps,
        })
    }

    /// WF1 — three-way merge of `a` (ours/first parent) and `b` (theirs).
    pub async fn merge(
        &self,
        repo_id: &str,
        a: &str,
        b: &str,
        author_name: &str,
        author_email: &str,
        message: Option<String>,
    ) -> Result<MergeOutcome, MergeError> {
        // SD5.2 entry pipeline order: is_skipped → parse → row probes →
        // bounds LAST.
        if self.db.is_skipped() {
            return Err(warned(MergeError::DbNotConfigured));
        }
        let a_id = parse_changeset_id(a)?;
        let b_id = parse_changeset_id(b)?;
        self.row_probe(repo_id, a_id.as_str()).await?;
        self.row_probe(repo_id, b_id.as_str()).await?;
        let message = message
            .unwrap_or_else(|| format!("merge {b} into {a}", b = b_id.as_str(), a = a_id.as_str()));
        check_inputs(author_name, author_email, &message)?;

        // Engine-owned ancestry BFS — merge_base's pick may be dominated and
        // is NEVER trusted for ancestry decisions (BR5.2).
        if self
            .is_ancestor(repo_id, a_id.as_str(), b_id.as_str())
            .await?
        {
            return Ok(MergeOutcome::FastForward { head: b_id });
        }
        if self
            .is_ancestor(repo_id, b_id.as_str(), a_id.as_str())
            .await?
        {
            return Ok(MergeOutcome::FastForward { head: a_id });
        }
        let base = self
            .index
            .merge_base(repo_id, a_id.as_str(), b_id.as_str())
            .await
            .map_err(map_index_err)?;
        let b_t = self.full_tree(repo_id, Some(base.as_str())).await?;
        let o_t = self.full_tree(repo_id, Some(a_id.as_str())).await?;
        let t_t = self.full_tree(repo_id, Some(b_id.as_str())).await?;
        let (prov_o, prov_t) = self
            .side_manifest_pair(
                repo_id,
                a_id.as_str(),
                b_id.as_str(),
                base.as_str(),
                &o_t,
                &t_t,
            )
            .await?;

        // PASS 1 — zero puts.
        let (merged, minted) = self
            .classify_all(&b_t, &o_t, &t_t, &prov_o, &prov_t)
            .await?;
        // Checkpoint: manifest relative to first parent `a` (BR5.1).
        let (changes, _emitted_olds) = manifest_diff(&o_t, &merged, &prov_t, &t_t);
        if changes.len() > self.caps.merge_paths_max {
            return Err(too_large());
        }
        // Deterministic create-input re-check — nothing caller-supplied may
        // abort after the first `put` (WF1.9).
        check_inputs(author_name, author_email, &message)?;

        // PASS 2 — conflict-free, inputs pre-validated.
        self.put_merged(repo_id, &b_t, &o_t, &t_t, &minted).await?;
        let changeset_id = self
            .store
            .create(CreateChangesetInput {
                repo_id: repo_id.to_string(),
                parents: vec![a_id.as_str().to_string(), b_id.as_str().to_string()],
                author_name: author_name.to_string(),
                author_email: author_email.to_string(),
                message,
                committed_at: None,
                changes,
            })
            .await
            .map_err(map_changeset_err)?;
        Ok(MergeOutcome::Merged { changeset_id })
    }

    /// WF5 — replay ONE changeset's changed files onto `new_parent` (chain
    /// replay is a client loop). Merge-node sources replay against
    /// `parents[0]`; extra parents drop (BR5.13).
    pub async fn rebase(
        &self,
        repo_id: &str,
        cs: &str,
        new_parent: &str,
        author_name: &str,
        author_email: &str,
        message: Option<String>,
    ) -> Result<MergeOutcome, MergeError> {
        if self.db.is_skipped() {
            return Err(warned(MergeError::DbNotConfigured));
        }
        let cs_id = parse_changeset_id(cs)?;
        let np_id = parse_changeset_id(new_parent)?;
        let cs_row = self.row_probe(repo_id, cs_id.as_str()).await?;
        self.row_probe(repo_id, np_id.as_str()).await?;
        let message = message.unwrap_or_else(|| cs_row.message.clone());
        check_inputs(author_name, author_email, &message)?;

        let parents = self.parents_of(repo_id, cs_id.as_str()).await?;
        let old_base = parents.into_iter().next();
        if old_base.as_deref() == Some(np_id.as_str()) {
            // Honest no-op — "already there", never "collapse" (BR5.14).
            return Ok(MergeOutcome::FastForward { head: cs_id });
        }
        // Descendant/cycle target — `invalid_input` is the honest kind.
        if self
            .is_ancestor(repo_id, cs_id.as_str(), np_id.as_str())
            .await?
        {
            return Err(invalid("new_parent", "is a descendant of the changeset"));
        }
        let o_t = self.full_tree(repo_id, Some(np_id.as_str())).await?;
        let t_t = self.full_tree(repo_id, Some(cs_id.as_str())).await?;
        // ∅ base for a root: synthesized empty map — never a `tree()` call.
        let b_t = self.full_tree(repo_id, old_base.as_deref()).await?;

        // Provenance from the SOURCE changeset's own manifest only — never a
        // target-side walk (reviewer 2).
        let rows = self
            .db
            .vcs_list_file_changes(cs_id.as_str())
            .await
            .map_err(internal)?;
        let raw = rename_rows(&rows);
        let prov = compose_provenance(&raw, &t_t);
        let prov_o = SideManifest::new();

        let (merged, minted) = self.classify_all(&b_t, &o_t, &t_t, &prov_o, &prov).await?;
        let (changes, _emitted_olds) = manifest_diff(&o_t, &merged, &prov, &t_t);
        if changes.len() > self.caps.merge_paths_max {
            return Err(too_large());
        }
        check_inputs(author_name, author_email, &message)?;
        self.put_merged(repo_id, &b_t, &o_t, &t_t, &minted).await?;
        let changeset_id = self
            .store
            .create(CreateChangesetInput {
                repo_id: repo_id.to_string(),
                parents: vec![np_id.as_str().to_string()],
                author_name: author_name.to_string(),
                author_email: author_email.to_string(),
                message,
                committed_at: None,
                changes,
            })
            .await
            .map_err(map_changeset_err)?;
        Ok(MergeOutcome::Merged { changeset_id })
    }

    // ------------------------------------------------------------------
    // Facade-level primitives
    // ------------------------------------------------------------------

    /// Presence probe — the manifest is never loaded for guards.
    async fn row_probe(&self, repo_id: &str, id: &str) -> Result<VcsChangesetRow, MergeError> {
        self.db
            .vcs_get_changeset(repo_id, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| warned(MergeError::NotFound))
    }

    async fn parents_of(&self, repo_id: &str, id: &str) -> Result<Vec<String>, MergeError> {
        self.db
            .vcs_changeset_parents(repo_id, id)
            .await
            .map_err(internal)
    }

    /// `tree()` cursor loop into a `BTreeMap` — `merge_paths_max` enforced
    /// DURING accumulation (BR5.26). `None` (rebase's ∅ base) returns an
    /// empty map WITHOUT calling `tree()` — there is no id to probe.
    async fn full_tree(&self, repo_id: &str, cs: Option<&str>) -> Result<TreeMap, MergeError> {
        let mut map = TreeMap::new();
        let Some(cs) = cs else { return Ok(map) };
        let mut cursor: Option<String> = None;
        loop {
            let (entries, next) = self
                .index
                .tree(repo_id, cs, None, MERGE_PAGE, cursor.as_deref())
                .await
                .map_err(map_index_err)?;
            for e in entries {
                map.insert(e.path.clone(), e);
                if map.len() > self.caps.merge_paths_max {
                    return Err(too_large());
                }
            }
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        Ok(map)
    }

    /// Engine-owned bounded ancestry BFS — `merge_base` may return a
    /// dominated ancestor and is never used for ancestry decisions (BR5.2).
    async fn is_ancestor(&self, repo_id: &str, anc: &str, desc: &str) -> Result<bool, MergeError> {
        if anc == desc {
            return Ok(true);
        }
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        visited.insert(desc.to_string());
        queue.push_back(desc.to_string());
        while let Some(x) = queue.pop_front() {
            for p in self.parents_of(repo_id, &x).await? {
                if p == anc {
                    return Ok(true);
                }
                if visited.insert(p.clone()) {
                    if visited.len() > self.caps.ancestor_probe_max {
                        return Err(too_large());
                    }
                    queue.push_back(p);
                }
            }
        }
        Ok(false)
    }

    // ------------------------------------------------------------------
    // Provenance
    // ------------------------------------------------------------------

    /// ONE shared mark phase (`base ∪ ancestors(base)`, ≤
    /// `ancestor_probe_max`) backing two side-exclusive walks (≤
    /// `side_walk_max` nodes each; collected `rename` rows ≤
    /// `rename_probe_max`) — the hashed-path `path_touch` grammar cannot
    /// enumerate per-side, so the ancestry walk is the mechanism (Q2).
    async fn side_manifest_pair(
        &self,
        repo_id: &str,
        tip_a: &str,
        tip_b: &str,
        base: &str,
        tree_a: &TreeMap,
        tree_b: &TreeMap,
    ) -> Result<(SideManifest, SideManifest), MergeError> {
        let mut marked: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        marked.insert(base.to_string());
        queue.push_back(base.to_string());
        while let Some(x) = queue.pop_front() {
            for p in self.parents_of(repo_id, &x).await? {
                if marked.insert(p.clone()) {
                    if marked.len() > self.caps.ancestor_probe_max {
                        return Err(too_large());
                    }
                    queue.push_back(p);
                }
            }
        }
        let raw_a = self.side_walk(repo_id, tip_a, &marked).await?;
        let raw_b = self.side_walk(repo_id, tip_b, &marked).await?;
        Ok((
            compose_provenance(&raw_a, tree_a),
            compose_provenance(&raw_b, tree_b),
        ))
    }

    /// Side-exclusive BFS from `tip` skipping marked nodes; collects the
    /// side's `rename` rows (old → new) across the whole walk.
    async fn side_walk(
        &self,
        repo_id: &str,
        tip: &str,
        marked: &HashSet<String>,
    ) -> Result<Vec<(String, String)>, MergeError> {
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut renames: Vec<(String, String)> = Vec::new();
        if !marked.contains(tip) {
            visited.insert(tip.to_string());
            queue.push_back(tip.to_string());
        }
        while let Some(x) = queue.pop_front() {
            let rows = self.db.vcs_list_file_changes(&x).await.map_err(internal)?;
            for r in &rows {
                if r.kind == ChangeKind::Rename.as_str() {
                    if let Some(old) = r.old_path.as_deref() {
                        renames.push((old.to_string(), r.path.clone()));
                        if renames.len() > self.caps.rename_probe_max {
                            return Err(too_large());
                        }
                    }
                }
            }
            for p in self.parents_of(repo_id, &x).await? {
                if marked.contains(&p) {
                    continue;
                }
                if visited.insert(p.clone()) {
                    if visited.len() > self.caps.side_walk_max {
                        return Err(too_large());
                    }
                    queue.push_back(p);
                }
            }
        }
        Ok(renames)
    }

    // ------------------------------------------------------------------
    // Classification — rename passes then the per-path table (PASS 1 shape:
    // zero `put`s; merged bytes are discarded after their mint is recorded).
    // ------------------------------------------------------------------

    async fn classify_all(
        &self,
        b_t: &TreeMap,
        o_t: &TreeMap,
        t_t: &TreeMap,
        prov_o: &SideManifest,
        prov_t: &SideManifest,
    ) -> Result<(TreeMap, BTreeMap<String, (i64, String)>), MergeError> {
        let mut conflicts: Vec<VcsConflictEntry> = Vec::new();
        let mut claimed: BTreeSet<String> = BTreeSet::new();
        let cap = self.caps.merge_conflict_max;

        // Cross-path rename/rename FIRST — claimed targets suppress per-path
        // re-emission (BR5.7).
        for (old, y) in prov_o {
            let Some(z) = prov_t.get(old) else { continue };
            if y != z {
                push_conflict(
                    &mut conflicts,
                    cap,
                    entry(
                        y,
                        ConflictKind::Rename,
                        Some(detail200(&format!("targets: {y}, {z}"))),
                    ),
                );
                claimed.insert(y.clone());
                claimed.insert(z.clone());
            }
        }
        // rename-vs-delete: `old` renamed by exactly one side, absent on the
        // other while present in base — never a silent keep (BR5.7).
        for (old, p) in prov_o {
            if prov_t.contains_key(old) {
                continue;
            }
            if b_t.contains_key(old) && !t_t.contains_key(old) {
                push_conflict(
                    &mut conflicts,
                    cap,
                    entry(
                        p,
                        ConflictKind::Rename,
                        Some(detail200(&format!(
                            "renamed {old} to {p}; deleted on theirs"
                        ))),
                    ),
                );
                claimed.insert(p.clone());
            }
        }
        for (old, p) in prov_t {
            if prov_o.contains_key(old) {
                continue;
            }
            if b_t.contains_key(old) && !o_t.contains_key(old) {
                push_conflict(
                    &mut conflicts,
                    cap,
                    entry(
                        p,
                        ConflictKind::Rename,
                        Some(detail200(&format!("renamed {old} to {p}; deleted on ours"))),
                    ),
                );
                claimed.insert(p.clone());
            }
        }

        // Union view for the rename-vs-modify `delete` detail.
        let mut prov_union = prov_o.clone();
        for (k, v) in prov_t {
            prov_union.entry(k.clone()).or_insert_with(|| v.clone());
        }

        let mut merged = TreeMap::new();
        let mut minted: BTreeMap<String, (i64, String)> = BTreeMap::new();
        let mut paths: BTreeSet<&str> = BTreeSet::new();
        paths.extend(b_t.keys().map(String::as_str));
        paths.extend(o_t.keys().map(String::as_str));
        paths.extend(t_t.keys().map(String::as_str));
        for path in paths {
            if claimed.contains(path) {
                continue;
            }
            if conflicts.len() > cap {
                break;
            }
            match self
                .classify_path(
                    path,
                    b_t.get(path),
                    o_t.get(path),
                    t_t.get(path),
                    &prov_union,
                    &mut minted,
                )
                .await?
            {
                Classified::Resolved(e) => {
                    if let Some(e) = e {
                        merged.insert(path.to_string(), e);
                    }
                }
                Classified::Conflict(c) => push_conflict(&mut conflicts, cap, c),
            }
        }
        if !conflicts.is_empty() {
            conflicts.sort_by(|x, y| x.path.cmp(&y.path));
            return Err(warned(MergeError::Conflict(conflicts)));
        }
        Ok((merged, minted))
    }

    /// WF2 — pick rules → `delete` → `type` → `mode` → blob merge3.
    async fn classify_path(
        &self,
        path: &str,
        b: Option<&TreeEntry>,
        o: Option<&TreeEntry>,
        t: Option<&TreeEntry>,
        prov: &SideManifest,
        minted: &mut BTreeMap<String, (i64, String)>,
    ) -> Result<Classified, MergeError> {
        if ent_eq(o, t) {
            return Ok(Classified::Resolved(o.cloned()));
        }
        if ent_eq(o, b) {
            return Ok(Classified::Resolved(t.cloned()));
        }
        if ent_eq(t, b) {
            return Ok(Classified::Resolved(o.cloned()));
        }
        let (Some(oe), Some(te)) = (o, t) else {
            // One side absent — delete stands alone; the detail names a
            // rename target when the path moved (rename-vs-modify).
            let detail = prov
                .get(path)
                .map(|target| detail200(&format!("renamed to {target}")));
            return Ok(Classified::Conflict(entry(
                path,
                ConflictKind::Delete,
                detail,
            )));
        };
        if is_nonregular(oe.mode) || is_nonregular(te.mode) {
            return Ok(Classified::Conflict(entry(path, ConflictKind::Type, None)));
        }
        if oe.mode != te.mode {
            return Ok(Classified::Conflict(entry(path, ConflictKind::Mode, None)));
        }
        if oe.blob_hash == te.blob_hash {
            // Equal (mode, blob) would have resolved at the first pick arm.
            return Ok(Classified::Resolved(Some(oe.clone())));
        }
        let base_bytes = match b {
            Some(be) => self.read_blob_capped(&be.blob_hash).await?,
            None => Vec::new(),
        };
        let o_bytes = self.read_blob_capped(&oe.blob_hash).await?;
        let t_bytes = self.read_blob_capped(&te.blob_hash).await?;
        if !is_text(&o_bytes) || !is_text(&t_bytes) {
            return Ok(Classified::Conflict(entry(
                path,
                ConflictKind::Binary,
                None,
            )));
        }
        let res = run_merge3(base_bytes, o_bytes, t_bytes)
            .await
            .map_err(|e| internal(format!("merge3 task: {e}")))?;
        let merged_bytes = match res {
            Ok(bytes) => bytes,
            Err(Merge3Fail::TooLarge) => return Err(too_large()),
            Err(Merge3Fail::Regions(regions)) => {
                return Ok(Classified::Conflict(entry(
                    path,
                    ConflictKind::Content,
                    Some(format!("regions: {}", regions.len())),
                )));
            }
        };
        if merged_bytes.len() as u64 > self.caps.merge_blob_max {
            return Err(too_large());
        }
        // Content-derived mint — the same hash `put` computes (BR5.3: the
        // bytes are discarded; pass 2 re-derives and asserts equality).
        let mint = local_blob_id(&merged_bytes);
        minted.insert(path.to_string(), (oe.mode, mint.clone()));
        Ok(Classified::Resolved(Some(TreeEntry {
            path: path.to_string(),
            mode: oe.mode,
            blob_hash: mint,
        })))
    }

    /// PASS 2 — only reachable post-checkpoint: re-read all three inputs (no
    /// pass-1 input caching), re-run `merge3` (deterministic → identical
    /// bytes → identical id), `put`, assert the mint.
    async fn put_merged(
        &self,
        repo_id: &str,
        b_t: &TreeMap,
        o_t: &TreeMap,
        t_t: &TreeMap,
        minted: &BTreeMap<String, (i64, String)>,
    ) -> Result<(), MergeError> {
        for (path, (_mode, mint)) in minted {
            let base_bytes = match b_t.get(path) {
                Some(be) => self.read_blob_capped(&be.blob_hash).await?,
                None => Vec::new(),
            };
            let oe = o_t
                .get(path)
                .ok_or_else(|| internal("pass-2 ours entry missing".to_string()))?;
            let te = t_t
                .get(path)
                .ok_or_else(|| internal("pass-2 theirs entry missing".to_string()))?;
            let o_bytes = self.read_blob_capped(&oe.blob_hash).await?;
            let t_bytes = self.read_blob_capped(&te.blob_hash).await?;
            let res = run_merge3(base_bytes, o_bytes, t_bytes)
                .await
                .map_err(|e| internal(format!("merge3 task: {e}")))?;
            let bytes = match res {
                Ok(bytes) => bytes,
                // Pass 1 succeeded on identical inputs — a failure here is
                // divergence, not a conflict.
                Err(_) => return Err(internal("pass-2 merge3 diverged".to_string())),
            };
            if bytes.len() as u64 > self.caps.merge_blob_max {
                return Err(too_large());
            }
            let outcome = self
                .objects
                .put(
                    stream::iter([Ok::<Bytes, Infallible>(Bytes::from(bytes))]),
                    repo_id,
                )
                .await
                .map_err(map_object_err)?;
            if outcome.hash != *mint {
                return Err(internal(
                    "pass-2 blob hash diverged from pass-1 mint".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// `take(merge_blob_max + 1)` read — over ⇒ `TooLarge` BEFORE the text
    /// test (BR5.21); `io::Error` is the fourth error channel ⇒ `Internal`.
    async fn read_blob_capped(&self, hash: &str) -> Result<Vec<u8>, MergeError> {
        let file = self.objects.get(hash).await.map_err(map_object_err)?;
        let mut limited = file.take(self.caps.merge_blob_max.saturating_add(1));
        let mut buf = Vec::new();
        limited
            .read_to_end(&mut buf)
            .await
            .map_err(|e| internal(e.to_string()))?;
        if buf.len() as u64 > self.caps.merge_blob_max {
            return Err(too_large());
        }
        Ok(buf)
    }
}

// ---------------------------------------------------------------------------
// Provenance composition
// ---------------------------------------------------------------------------

/// `(old_path, new_path)` pairs out of one changeset's manifest rows.
fn rename_rows(rows: &[oxidean_db::VcsFileChangeRow]) -> Vec<(String, String)> {
    rows.iter()
        .filter(|r| r.kind == ChangeKind::Rename.as_str())
        .filter_map(|r| r.old_path.clone().map(|o| (o, r.path.clone())))
        .collect()
}

/// Transitive chain compose (`old→p→q` ⇒ `prov[old]=q`); cycles and
/// multi-writer (distinct-target) `old`s dropped — ambiguous ⇒ no
/// provenance; final target must exist in the side's tip tree (BR5.17).
fn compose_provenance(raw: &[(String, String)], tip_tree: &TreeMap) -> SideManifest {
    let mut distinct: BTreeMap<&str, HashSet<&str>> = BTreeMap::new();
    for (old, new) in raw {
        distinct
            .entry(old.as_str())
            .or_default()
            .insert(new.as_str());
    }
    let mut step: BTreeMap<String, String> = BTreeMap::new();
    for (old, set) in &distinct {
        if set.len() == 1 {
            step.insert(
                (*old).to_string(),
                set.iter().next().expect("len checked").to_string(),
            );
        }
    }
    let mut prov = SideManifest::new();
    let mut dropped: HashSet<String> = HashSet::new();
    for old in step.keys() {
        if dropped.contains(old) {
            continue;
        }
        let mut chain = vec![old.clone()];
        let target = loop {
            let cur = chain.last().expect("nonempty").clone();
            match step.get(&cur) {
                None => break Some(cur),
                Some(next) => {
                    if chain.iter().any(|c| c == next) || dropped.contains(next) {
                        for c in &chain {
                            dropped.insert(c.clone());
                        }
                        break None;
                    }
                    chain.push(next.clone());
                }
            }
        };
        if let Some(t) = target {
            if tip_tree.contains_key(&t) {
                prov.insert(old.clone(), t);
            }
        }
    }
    prov
}

// ---------------------------------------------------------------------------
// manifest_diff — merged tree → `FileChangeInput`s relative to the FIRST
// parent (BR5.23). Output unsorted — `canonical_changeset` owns the sort.
// ---------------------------------------------------------------------------

/// Returns `(changes, emitted_rename_olds)` — the old set must be fully
/// populated BEFORE the delete arm evaluates, else every emitted rename
/// would also mint a phantom `delete{old}` falsifying `path_touch`/`blame`.
fn manifest_diff(
    first: &TreeMap,
    merged: &TreeMap,
    prov: &SideManifest,
    prov_tree: &TreeMap,
) -> (Vec<FileChangeInput>, BTreeSet<String>) {
    // Byte-first `old` wins a multi-source target (BTreeMap order).
    let mut by_target: BTreeMap<&str, &str> = BTreeMap::new();
    for (old, target) in prov {
        by_target.entry(target.as_str()).or_insert(old.as_str());
    }
    let mut changes = Vec::new();
    let mut emitted_olds: BTreeSet<String> = BTreeSet::new();
    for (p, e) in merged {
        if first.contains_key(p) {
            continue;
        }
        let rename_old = by_target.get(p.as_str()).copied().filter(|old| {
            !merged.contains_key(*old)
                && first.contains_key(*old)
                && prov_tree
                    .get(p)
                    .is_some_and(|pt| pt.mode == e.mode && pt.blob_hash == e.blob_hash)
        });
        match rename_old {
            Some(old) => {
                changes.push(FileChangeInput {
                    path: p.clone(),
                    kind: ChangeKind::Rename,
                    mode: e.mode,
                    old_path: Some(old.to_string()),
                    blob_hash: Some(e.blob_hash.clone()),
                });
                emitted_olds.insert(old.to_string());
            }
            None => changes.push(FileChangeInput {
                path: p.clone(),
                kind: ChangeKind::Add,
                mode: e.mode,
                old_path: None,
                blob_hash: Some(e.blob_hash.clone()),
            }),
        }
    }
    for (p, e) in first {
        if merged.contains_key(p) || emitted_olds.contains(p) {
            continue;
        }
        changes.push(FileChangeInput {
            path: p.clone(),
            kind: ChangeKind::Delete,
            mode: e.mode,
            old_path: None,
            blob_hash: None,
        });
    }
    for (p, e) in merged {
        let Some(f) = first.get(p) else { continue };
        if f.mode != e.mode || f.blob_hash != e.blob_hash {
            changes.push(FileChangeInput {
                path: p.clone(),
                kind: ChangeKind::Modify,
                mode: e.mode,
                old_path: None,
                blob_hash: Some(e.blob_hash.clone()),
            });
        }
    }
    (changes, emitted_olds)
}

// ---------------------------------------------------------------------------
// merge3 — byte-faithful three-way line merge (SD5.6).
// ---------------------------------------------------------------------------

/// One side's decoded buffer: terminatorless lines plus the trailing-newline
/// flag (`split_lines` drops both — they must ride alongside).
struct SideBuf {
    lines: Vec<Vec<u8>>,
    ends_nl: bool,
    /// Per base boundary `p ∈ 0..=n_base`: first/last covering match-region
    /// side position (`None` inside hunk interiors). `min`/`max` differ
    /// exactly at insertion boundaries — `min` is pre-insertion, `max`
    /// post-insertion.
    pos_min: Vec<Option<usize>>,
    pos_max: Vec<usize>,
}

impl SideBuf {
    fn build(lines: Vec<Vec<u8>>, ends_nl: bool, n_base: usize, hunks: &[LineHunk]) -> Self {
        let matches = match_regions(n_base, hunks);
        let (pos_min, pos_max) = boundary_maps(&matches, n_base);
        Self {
            lines,
            ends_nl,
            pos_min,
            pos_max,
        }
    }
}

/// Line `g`'s terminator flag — `i < len-1 || ends_nl` (WF4.3).
fn term(side: &SideBuf, g: usize) -> bool {
    g + 1 < side.lines.len() || side.ends_nl
}

/// `(line_bytes, ends_nl)` pair equality — the last line's terminator joins
/// the sameness test (trailing-newline-only divergence is a real conflict).
fn slice_eq(x: &SideBuf, xr: Range<usize>, y: &SideBuf, yr: Range<usize>) -> bool {
    if xr.len() != yr.len() {
        return false;
    }
    (0..xr.len()).all(|k| {
        let (xi, yi) = (xr.start + k, yr.start + k);
        x.lines[xi] == y.lines[yi] && term(x, xi) == term(y, yi)
    })
}

fn emit(out: &mut Vec<u8>, side: &SideBuf, range: Range<usize>) {
    for g in range {
        out.extend_from_slice(&side.lines[g]);
        if term(side, g) {
            out.push(b'\n');
        }
    }
}

/// The three decode buffers handed to each region classification.
struct Sides<'a> {
    base: &'a SideBuf,
    ours: &'a SideBuf,
    theirs: &'a SideBuf,
}

/// One base-region slice and its mapped ours/theirs spans.
struct Region {
    b: Range<usize>,
    o: Range<usize>,
    t: Range<usize>,
}

/// Complement of `hunks`: match regions `(z_start, z_end, side_start)` —
/// zero-length regions kept (they carry boundary-sync at hunk edges).
fn match_regions(n_base: usize, hunks: &[LineHunk]) -> Vec<(usize, usize, usize)> {
    let mut out = Vec::with_capacity(hunks.len() + 1);
    let mut zb = 0usize;
    let mut sb = 0usize;
    for h in hunks {
        out.push((zb, h.old_start, sb));
        zb = h.old_start + h.old_len;
        sb = h.new_start + h.new_len;
    }
    out.push((zb, n_base, sb));
    out
}

fn boundary_maps(
    matches: &[(usize, usize, usize)],
    n_base: usize,
) -> (Vec<Option<usize>>, Vec<usize>) {
    let mut lo = vec![None; n_base + 1];
    let mut hi = vec![0usize; n_base + 1];
    for &(z0, z1, s0) in matches {
        for p in z0..=z1 {
            let v = s0 + (p - z0);
            if lo[p].is_none() {
                lo[p] = Some(v);
            }
            hi[p] = v;
        }
    }
    (lo, hi)
}

/// Classify one base region and emit (or record) its merged bytes. `tail`
/// regions close on `pos_max` so insertions at `n_base` land inside them.
fn region_step(
    out: &mut Vec<u8>,
    conflicts: &mut Vec<ConflictRegion>,
    sides: &Sides<'_>,
    region: &Region,
) {
    let same_o = slice_eq(sides.base, region.b.clone(), sides.ours, region.o.clone());
    let same_t = slice_eq(sides.base, region.b.clone(), sides.theirs, region.t.clone());
    if same_o && same_t {
        emit(out, sides.base, region.b.clone());
    } else if same_o {
        emit(out, sides.theirs, region.t.clone());
    } else if same_t || slice_eq(sides.ours, region.o.clone(), sides.theirs, region.t.clone()) {
        // Both-equal regions and "theirs kept base" emit `ours`; when the
        // side slices coincide the byte output is identical either way.
        emit(out, sides.ours, region.o.clone());
    } else {
        conflicts.push(ConflictRegion {
            ours_range: region.o.clone(),
            theirs_range: region.t.clone(),
        });
    }
}

/// Pure three-way merge — deterministic in (base, ours, theirs); runs inside
/// `run_merge3`'s `spawn_blocking` seam, never inline in an `async` body.
fn merge3(base: Vec<u8>, ours: Vec<u8>, theirs: Vec<u8>) -> Result<Vec<u8>, Merge3Fail> {
    // Byte-count `b'\n'` BEFORE `split_lines` materializes ~4M line vecs
    // (devsecops 10; iter filter — memchr is not a dep). The +2 covers
    // `n+1` lines per unterminated buffer and guarantees `diff_lines`' own
    // coarse fallback is unreachable from here.
    let n_b = base.iter().filter(|&&b| b == b'\n').count();
    let n_o = ours.iter().filter(|&&b| b == b'\n').count();
    let n_t = theirs.iter().filter(|&&b| b == b'\n').count();
    if n_b + n_o + 2 > LINE_CAP || n_b + n_t + 2 > LINE_CAP {
        return Err(Merge3Fail::TooLarge);
    }

    let b_lines = split_lines(&base);
    let o_lines = split_lines(&ours);
    let t_lines = split_lines(&theirs);
    let n_base = b_lines.len();
    let b_refs: Vec<&[u8]> = b_lines.iter().map(Vec::as_slice).collect();
    let o_refs: Vec<&[u8]> = o_lines.iter().map(Vec::as_slice).collect();
    let t_refs: Vec<&[u8]> = t_lines.iter().map(Vec::as_slice).collect();
    let hunks_o = diff_lines(&b_refs, &o_refs);
    let hunks_t = diff_lines(&b_refs, &t_refs);

    let base_side = SideBuf {
        lines: b_lines,
        ends_nl: base.last() == Some(&b'\n'),
        pos_min: Vec::new(),
        pos_max: Vec::new(),
    };
    let ours_side = SideBuf::build(o_lines, ours.last() == Some(&b'\n'), n_base, &hunks_o);
    let theirs_side = SideBuf::build(t_lines, theirs.last() == Some(&b'\n'), n_base, &hunks_t);

    // Both-match boundaries — insertions attach to the region STARTING at
    // their boundary (`pos_min` at both edges); the synthesized tail
    // `[n_base, n_base]` absorbs insertions at end-of-buffer via `pos_max`.
    let bounds: Vec<usize> = (0..=n_base)
        .filter(|&p| ours_side.pos_min[p].is_some() && theirs_side.pos_min[p].is_some())
        .collect();
    let sides = Sides {
        base: &base_side,
        ours: &ours_side,
        theirs: &theirs_side,
    };
    let mut out: Vec<u8> = Vec::new();
    let mut conflicts: Vec<ConflictRegion> = Vec::new();
    for w in bounds.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        let region = Region {
            b: lo..hi,
            o: ours_side.pos_min[lo].expect("both-match boundary")
                ..ours_side.pos_min[hi].expect("both-match boundary"),
            t: theirs_side.pos_min[lo].expect("both-match boundary")
                ..theirs_side.pos_min[hi].expect("both-match boundary"),
        };
        region_step(&mut out, &mut conflicts, &sides, &region);
    }
    // Tail — `pos_min[n]` (pre-insertion) .. `pos_max[n]` (post-insertion).
    let tail = Region {
        b: n_base..n_base,
        o: ours_side.pos_min[n_base].unwrap_or(0)..ours_side.pos_max[n_base],
        t: theirs_side.pos_min[n_base].unwrap_or(0)..theirs_side.pos_max[n_base],
    };
    region_step(&mut out, &mut conflicts, &sides, &tail);
    if !conflicts.is_empty() {
        return Err(Merge3Fail::Regions(conflicts));
    }
    Ok(out)
}

/// The BR5.10 seam — `merge3` on the blocking pool. Test reachability
/// (u5.39) requires the private `JoinHandle` return, not an `async fn`.
fn run_merge3(
    base: Vec<u8>,
    ours: Vec<u8>,
    theirs: Vec<u8>,
) -> JoinHandle<Result<Vec<u8>, Merge3Fail>> {
    tokio::task::spawn_blocking(move || merge3(base, ours, theirs))
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::object_store::testutil::{cfg, seed_repo, shard_file, store_unchecked, test_db};
    use crate::object_store::ObjectStoreConfig;
    use crate::read_index::ReadIndexCaps;

    struct Rig {
        _dbdir: tempfile::TempDir,
        odir: tempfile::TempDir,
        db: Database,
        repo: String,
        engine: MergeEngine,
        store: ChangesetStore,
        objects: ObjectStore,
    }

    async fn rig(caps: MergeCaps) -> Rig {
        let (dbdir, db) = test_db().await;
        let repo = seed_repo(&db, "u-me", "r-me").await;
        let odir = tempfile::tempdir().unwrap();
        let c = cfg(odir.path());
        let engine_objects = ObjectStore::new(db.clone(), c.clone()).unwrap();
        let index = ReadIndex::new(
            db.clone(),
            store_unchecked(db.clone(), c.clone()),
            ReadIndexCaps::default(),
        );
        let engine = MergeEngine::new(
            db.clone(),
            index,
            ChangesetStore::new(db.clone()),
            engine_objects,
            caps,
        )
        .unwrap();
        Rig {
            _dbdir: dbdir,
            odir,
            db: db.clone(),
            repo,
            engine,
            store: ChangesetStore::new(db.clone()),
            objects: store_unchecked(db, c),
        }
    }

    fn add(path: &str, hash: &str) -> FileChangeInput {
        FileChangeInput {
            path: path.into(),
            kind: ChangeKind::Add,
            mode: 33188,
            old_path: None,
            blob_hash: Some(hash.into()),
        }
    }

    fn modify(path: &str, hash: &str) -> FileChangeInput {
        FileChangeInput {
            path: path.into(),
            kind: ChangeKind::Modify,
            mode: 33188,
            old_path: None,
            blob_hash: Some(hash.into()),
        }
    }

    fn modify_mode(path: &str, mode: i64, hash: &str) -> FileChangeInput {
        FileChangeInput {
            mode,
            ..modify(path, hash)
        }
    }

    fn del(path: &str) -> FileChangeInput {
        FileChangeInput {
            path: path.into(),
            kind: ChangeKind::Delete,
            mode: 33188,
            old_path: None,
            blob_hash: None,
        }
    }

    fn rename(old: &str, path: &str, hash: &str) -> FileChangeInput {
        FileChangeInput {
            path: path.into(),
            kind: ChangeKind::Rename,
            mode: 33188,
            old_path: Some(old.into()),
            blob_hash: Some(hash.into()),
        }
    }

    /// Seed an object row + shard bytes so `create` blob validation and
    /// `objects.get` both work; no link row (quota-neutral for tests).
    async fn blob(r: &Rig, bytes: &[u8]) -> String {
        let hash = local_blob_id(bytes);
        r.db.vcs_insert_object_if_absent(&hash, bytes.len() as i64)
            .await
            .unwrap();
        shard_file(r.odir.path(), &hash, bytes);
        hash
    }

    async fn cs(r: &Rig, parents: &[&str], msg: &str, changes: Vec<FileChangeInput>) -> String {
        r.store
            .create(CreateChangesetInput {
                repo_id: r.repo.clone(),
                parents: parents.iter().map(|p| p.to_string()).collect(),
                author_name: "T".into(),
                author_email: "t@example.com".into(),
                message: msg.into(),
                committed_at: None,
                changes,
            })
            .await
            .unwrap_or_else(|e| panic!("create {msg}: {e}"))
            .as_str()
            .to_string()
    }

    async fn blob_bytes(r: &Rig, hash: &str) -> Vec<u8> {
        let mut f = r.objects.get(hash).await.unwrap();
        let mut v = Vec::new();
        f.read_to_end(&mut v).await.unwrap();
        v
    }

    fn merged_id(out: MergeOutcome) -> String {
        match out {
            MergeOutcome::Merged { changeset_id } => changeset_id.as_str().to_string(),
            other => panic!("expected Merged: {other:?}"),
        }
    }

    fn ff_head(out: MergeOutcome) -> String {
        match out {
            MergeOutcome::FastForward { head } => head.as_str().to_string(),
            other => panic!("expected FastForward: {other:?}"),
        }
    }

    fn conflicts_of(res: Result<MergeOutcome, MergeError>) -> Vec<VcsConflictEntry> {
        match res {
            Err(MergeError::Conflict(entries)) => entries,
            other => panic!("expected Conflict: {other:?}"),
        }
    }

    fn te(path: &str, hash: &str) -> TreeEntry {
        TreeEntry {
            path: path.into(),
            mode: 33188,
            blob_hash: hash.into(),
        }
    }

    fn tree(entries: Vec<TreeEntry>) -> TreeMap {
        entries.into_iter().map(|e| (e.path.clone(), e)).collect()
    }

    // ---------------------------------------------------------------
    // merge
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn fast_forward_b() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let a = cs(&r, &[], "a", vec![add("f", &h)]).await;
        let b = cs(&r, &[&a], "b", vec![add("g", &h)]).await;
        let before = r.db.vcs_count_changesets(&r.repo).await.unwrap();
        let out = r
            .engine
            .merge(&r.repo, &a, &b, "T", "t@example.com", None)
            .await
            .unwrap();
        assert_eq!(ff_head(out), b);
        assert_eq!(
            r.db.vcs_count_changesets(&r.repo).await.unwrap(),
            before,
            "fast path writes zero rows"
        );
    }

    #[tokio::test]
    async fn fast_forward_a() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let b = cs(&r, &[], "b", vec![add("f", &h)]).await;
        let a = cs(&r, &[&b], "a", vec![add("g", &h)]).await;
        let before = r.db.vcs_count_changesets(&r.repo).await.unwrap();
        let out = r
            .engine
            .merge(&r.repo, &a, &b, "T", "t@example.com", None)
            .await
            .unwrap();
        assert_eq!(ff_head(out), a);
        assert_eq!(r.db.vcs_count_changesets(&r.repo).await.unwrap(), before);
    }

    #[tokio::test]
    async fn equal_heads() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let a = cs(&r, &[], "a", vec![add("f", &h)]).await;
        let before = r.db.vcs_count_changesets(&r.repo).await.unwrap();
        let out = r
            .engine
            .merge(&r.repo, &a, &a, "T", "t@example.com", None)
            .await
            .unwrap();
        assert_eq!(ff_head(out), a);
        assert_eq!(r.db.vcs_count_changesets(&r.repo).await.unwrap(), before);
    }

    #[tokio::test]
    async fn clean_merge() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"l1\nl2\nl3\nl4\nl5\n").await;
        let ho = blob(&r, b"O\nl2\nl3\nl4\nl5\n").await;
        let ht = blob(&r, b"l1\nl2\nl3\nl4\nT\n").await;
        let base = cs(&r, &[], "base", vec![add("f.txt", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![modify("f.txt", &ho)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f.txt", &ht)]).await;
        let out = r
            .engine
            .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
            .await
            .unwrap();
        let id = merged_id(out);
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        assert_eq!(
            rec.parents.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            vec![ours.as_str(), theirs.as_str()],
            "merge parents are [a, b]"
        );
        // Manifest is relative to the FIRST parent (ours) — only f.txt
        // changed relative to it.
        assert_eq!(rec.manifest.len(), 1);
        assert_eq!(rec.manifest[0].path, "f.txt");
        assert_eq!(rec.manifest[0].kind, ChangeKind::Modify);
        let merged_blob = rec.manifest[0].blob_hash.clone().unwrap();
        assert_eq!(
            blob_bytes(&r, &merged_blob).await,
            b"O\nl2\nl3\nl4\nT\n",
            "disjoint line edits merge byte-exact"
        );
        assert!(
            r.db.vcs_get_object_link(&r.repo, &merged_blob)
                .await
                .unwrap(),
            "pass-2 put linked the merged blob"
        );
    }

    #[tokio::test]
    async fn merge_idempotent() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"a\nb\n").await;
        let ho = blob(&r, b"a\nO\n").await;
        let ht = blob(&r, b"T\nb\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![modify("f", &ho)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f", &ht)]).await;
        let id1 = merged_id(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let n = r.db.vcs_count_changesets(&r.repo).await.unwrap();
        let id2 = merged_id(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        assert_eq!(id1, id2);
        assert_eq!(r.db.vcs_count_changesets(&r.repo).await.unwrap(), n);
    }

    #[tokio::test]
    async fn merge_message_default() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &h)]).await;
        let ours = cs(&r, &[&base], "ours", vec![add("o", &h)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![add("t", &h)]).await;
        let id = merged_id(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        assert_eq!(rec.message, format!("merge {theirs} into {ours}"));
        let id2 = merged_id(
            r.engine
                .merge(
                    &r.repo,
                    &ours,
                    &theirs,
                    "T",
                    "t@example.com",
                    Some("explicit msg".into()),
                )
                .await
                .unwrap(),
        );
        let rec2 = r.store.get(&r.repo, &id2).await.unwrap();
        assert_eq!(rec2.message, "explicit msg");
    }

    #[tokio::test]
    async fn no_common_ancestor() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let a = cs(&r, &[], "a", vec![add("f", &h)]).await;
        let b = cs(&r, &[], "b", vec![add("g", &h)]).await;
        let res = r
            .engine
            .merge(&r.repo, &a, &b, "T", "t@example.com", None)
            .await;
        assert!(
            matches!(res, Err(MergeError::NoCommonAncestor)),
            "disjoint histories: {res:?}"
        );
    }

    #[tokio::test]
    async fn conflict_content() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"a\nb\nc\n").await;
        let ho = blob(&r, b"a\nB1\nc\n").await;
        let ht = blob(&r, b"a\nB2\nc\n").await;
        let base = cs(&r, &[], "base", vec![add("f.txt", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![modify("f.txt", &ho)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f.txt", &ht)]).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "f.txt");
        assert_eq!(entries[0].kind, "content");
        assert_eq!(entries[0].detail.as_deref(), Some("regions: 1"));
    }

    #[tokio::test]
    async fn conflict_binary() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"a\nb\n").await;
        let ho = blob(&r, b"a\x00b\n").await;
        let ht = blob(&r, b"a\nc\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![modify("f", &ho)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f", &ht)]).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "f");
        assert_eq!(entries[0].kind, "binary");
    }

    #[tokio::test]
    async fn conflict_mode() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"x\n").await;
        let ht = blob(&r, b"y\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![modify_mode("f", 33261, &hb)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f", &ht)]).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "mode");
    }

    #[tokio::test]
    async fn conflict_type() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"x\n").await;
        let hl = blob(&r, b"target").await;
        let ht = blob(&r, b"y\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &hb)]).await;
        // ours re-types f to a symlink blob; theirs edits content.
        let ours = cs(&r, &[&base], "ours", vec![modify_mode("f", 40960, &hl)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f", &ht)]).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, "type");
    }

    #[tokio::test]
    async fn conflict_delete() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"x\n").await;
        let ht = blob(&r, b"y\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![del("f")]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f", &ht)]).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "f");
        assert_eq!(entries[0].kind, "delete");
        assert!(entries[0].detail.is_none());
    }

    #[tokio::test]
    async fn conflict_rename_rename() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"x\n").await;
        let base = cs(&r, &[], "base", vec![add("old.txt", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![rename("old.txt", "y.txt", &hb)]).await;
        let theirs = cs(
            &r,
            &[&base],
            "theirs",
            vec![rename("old.txt", "z.txt", &hb)],
        )
        .await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1, "claimed targets suppress re-emission");
        assert_eq!(entries[0].path, "y.txt");
        assert_eq!(entries[0].kind, "rename");
        let detail = entries[0].detail.as_deref().unwrap();
        assert!(detail.contains("y.txt") && detail.contains("z.txt"));
    }

    #[tokio::test]
    async fn conflict_rename_delete() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"x\n").await;
        let base = cs(&r, &[], "base", vec![add("old.txt", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![rename("old.txt", "y.txt", &hb)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![del("old.txt")]).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "y.txt");
        assert_eq!(entries[0].kind, "rename");
        let detail = entries[0].detail.as_deref().unwrap();
        assert!(detail.contains("old.txt") && detail.contains("theirs"));
    }

    #[tokio::test]
    async fn conflict_rename_modify() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"x\n").await;
        let ht = blob(&r, b"y\n").await;
        let base = cs(&r, &[], "base", vec![add("old.txt", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![rename("old.txt", "y.txt", &hb)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("old.txt", &ht)]).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        // rename-vs-modify stays `delete` kind on `old`, detail names target.
        assert_eq!(entries[0].path, "old.txt");
        assert_eq!(entries[0].kind, "delete");
        assert!(entries[0].detail.as_deref().unwrap().contains("y.txt"));
    }

    #[tokio::test]
    async fn conflict_order_and_cap() {
        let caps = MergeCaps {
            merge_conflict_max: 2,
            ..MergeCaps::default()
        };
        let r = rig(caps).await;
        let hb = blob(&r, b"a\nb\n").await;
        let mut base_changes = Vec::new();
        for i in 0..3 {
            base_changes.push(add(&format!("p{i}"), &hb));
        }
        let base = cs(&r, &[], "base", base_changes).await;
        let ho = blob(&r, b"a\nO\n").await;
        let ht = blob(&r, b"a\nT\n").await;
        let mut ours_changes = Vec::new();
        let mut theirs_changes = Vec::new();
        for i in 0..3 {
            ours_changes.push(modify(&format!("p{i}"), &ho));
            theirs_changes.push(modify(&format!("p{i}"), &ht));
        }
        let ours = cs(&r, &[&base], "ours", ours_changes).await;
        let theirs = cs(&r, &[&base], "theirs", theirs_changes).await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        // merge_conflict_max + 1 collected (drives the wire `truncated` flag).
        assert_eq!(entries.len(), 3);
        let mut sorted = entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>();
        sorted.sort();
        assert_eq!(
            entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>(),
            sorted.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            "conflict entries are path-sorted"
        );
        assert!(entries.iter().all(|e| e.kind == "content"));
    }

    #[tokio::test]
    async fn detail_truncated_200() {
        let r = rig(MergeCaps::default()).await;
        let hb = blob(&r, b"x\n").await;
        let long_target = format!("d/{}", "x".repeat(300));
        let base = cs(&r, &[], "base", vec![add("old.txt", &hb)]).await;
        let ours = cs(
            &r,
            &[&base],
            "ours",
            vec![rename("old.txt", &long_target, &hb)],
        )
        .await;
        let theirs = cs(
            &r,
            &[&base],
            "theirs",
            vec![rename("old.txt", "z.txt", &hb)],
        )
        .await;
        let entries = conflicts_of(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await,
        );
        assert_eq!(entries.len(), 1);
        let detail = entries[0].detail.as_deref().unwrap();
        let full = format!("targets: {long_target}, z.txt");
        assert!(full.chars().count() > 200);
        assert_eq!(detail.chars().count(), 200);
        assert_eq!(detail, full.chars().take(200).collect::<String>());
    }

    #[tokio::test]
    async fn zero_write_on_conflict() {
        let r = rig(MergeCaps::default()).await;
        let hb_f = blob(&r, b"a\nb\nc\n").await;
        let hb_g = blob(&r, b"1\n2\n3\n4\n5\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &hb_f), add("g", &hb_g)]).await;
        // g merges clean (disjoint edits) — pass 1 records a minted blob;
        // f conflicts — the whole call aborts with zero writes.
        let ho_f = blob(&r, b"a\nM1\nc\n").await;
        let ht_f = blob(&r, b"a\nM2\nc\n").await;
        let ho_g = blob(&r, b"O\n2\n3\n4\n5\n").await;
        let ht_g = blob(&r, b"1\n2\n3\n4\nT\n").await;
        let ours = cs(
            &r,
            &[&base],
            "ours",
            vec![modify("f", &ho_f), modify("g", &ho_g)],
        )
        .await;
        let theirs = cs(
            &r,
            &[&base],
            "theirs",
            vec![modify("f", &ht_f), modify("g", &ht_g)],
        )
        .await;
        let merged_g = merge3(
            b"1\n2\n3\n4\n5\n".to_vec(),
            b"O\n2\n3\n4\n5\n".to_vec(),
            b"1\n2\n3\n4\nT\n".to_vec(),
        )
        .unwrap();
        let mint = local_blob_id(&merged_g);
        let before_cs = r.db.vcs_count_changesets(&r.repo).await.unwrap();
        let before_obj =
            r.db.vcs_repo_object_counts(&r.repo)
                .await
                .unwrap()
                .object_count;
        let res = r
            .engine
            .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::Conflict(_))), "{res:?}");
        assert_eq!(
            r.db.vcs_count_changesets(&r.repo).await.unwrap(),
            before_cs,
            "conflict checkpoint precedes create — no changeset row"
        );
        assert_eq!(
            r.db.vcs_repo_object_counts(&r.repo)
                .await
                .unwrap()
                .object_count,
            before_obj,
            "no object rows written"
        );
        assert!(
            r.db.vcs_get_object(&mint).await.unwrap().is_none(),
            "pass-1 mint never reaches the object table"
        );
        assert!(
            !r.db.vcs_get_object_link(&r.repo, &mint).await.unwrap(),
            "no link row either"
        );
    }

    #[tokio::test]
    async fn zero_write_on_abort() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &h)]).await;
        let ours = cs(&r, &[&base], "ours", vec![add("o", &h)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![add("t", &h)]).await;
        let before_cs = r.db.vcs_count_changesets(&r.repo).await.unwrap();
        let before_obj =
            r.db.vcs_repo_object_counts(&r.repo)
                .await
                .unwrap()
                .object_count;
        let res = r
            .engine
            .merge(&r.repo, &ours, &theirs, "<bad", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::InvalidInput(_))), "{res:?}");
        assert_eq!(r.db.vcs_count_changesets(&r.repo).await.unwrap(), before_cs);
        assert_eq!(
            r.db.vcs_repo_object_counts(&r.repo)
                .await
                .unwrap()
                .object_count,
            before_obj
        );
    }

    // ---------------------------------------------------------------
    // provenance primitives (compose_provenance / manifest_diff direct)
    // ---------------------------------------------------------------

    #[test]
    fn rename_chain_compose() {
        let tip = tree(vec![te("C", "h")]);
        let raw = vec![
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "C".to_string()),
        ];
        let prov = compose_provenance(&raw, &tip);
        assert_eq!(prov.get("A").map(String::as_str), Some("C"));
        assert_eq!(prov.get("B").map(String::as_str), Some("C"));
    }

    #[test]
    fn rename_multiwriter_drop() {
        let tip = tree(vec![te("X", "h"), te("Y", "h")]);
        let raw = vec![
            ("A".to_string(), "X".to_string()),
            ("A".to_string(), "Y".to_string()),
        ];
        assert!(compose_provenance(&raw, &tip).is_empty());
    }

    #[test]
    fn rename_cycle_drop() {
        let tip = tree(vec![te("A", "h"), te("B", "h")]);
        let raw = vec![
            ("A".to_string(), "B".to_string()),
            ("B".to_string(), "A".to_string()),
        ];
        assert!(compose_provenance(&raw, &tip).is_empty());
    }

    #[test]
    fn rename_tip_filter() {
        let tip = tree(vec![te("C", "h")]);
        let raw = vec![("A".to_string(), "B".to_string())];
        assert!(
            compose_provenance(&raw, &tip).is_empty(),
            "final target absent from tip tree → filtered"
        );
        let tip2 = tree(vec![te("B", "h")]);
        assert_eq!(
            compose_provenance(&raw, &tip2).get("A").map(String::as_str),
            Some("B")
        );
    }

    #[test]
    fn rename_manifest_emission() {
        let first = tree(vec![te("old", "h1")]);
        let merged = tree(vec![te("p", "h1")]);
        let prov_tree = tree(vec![te("p", "h1")]);
        let prov: SideManifest = [("old".to_string(), "p".to_string())].into_iter().collect();
        let (changes, olds) = manifest_diff(&first, &merged, &prov, &prov_tree);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].kind, ChangeKind::Rename);
        assert_eq!(changes[0].path, "p");
        assert_eq!(changes[0].old_path.as_deref(), Some("old"));
        assert!(olds.contains("old"));

        // prov_tree divergence (content changed after rename) → the target
        // collapses to a plain add and `old` becomes an honest delete.
        let prov_tree_diff = tree(vec![te("p", "other")]);
        let (changes2, _) = manifest_diff(&first, &merged, &prov, &prov_tree_diff);
        assert_eq!(changes2.len(), 2);
        assert!(changes2
            .iter()
            .any(|c| c.kind == ChangeKind::Add && c.path == "p"));
        assert!(changes2
            .iter()
            .any(|c| c.kind == ChangeKind::Delete && c.path == "old"));

        // old absent from `first` → not provable → add.
        let first_empty = tree(vec![]);
        let (changes3, _) = manifest_diff(&first_empty, &merged, &prov, &prov_tree);
        assert_eq!(changes3[0].kind, ChangeKind::Add);
    }

    #[test]
    fn rename_byte_first_old() {
        let first = tree(vec![te("aaa", "h"), te("bbb", "h")]);
        let merged = tree(vec![te("p", "h")]);
        let prov_tree = tree(vec![te("p", "h")]);
        let prov: SideManifest = [
            ("bbb".to_string(), "p".to_string()),
            ("aaa".to_string(), "p".to_string()),
        ]
        .into_iter()
        .collect();
        let (changes, olds) = manifest_diff(&first, &merged, &prov, &prov_tree);
        let rename_row = changes
            .iter()
            .find(|c| c.kind == ChangeKind::Rename)
            .expect("one rename row");
        assert_eq!(
            rename_row.old_path.as_deref(),
            Some("aaa"),
            "byte-first old wins"
        );
        assert!(olds.contains("aaa"));
        // The losing old is unclaimed → an honest delete row remains.
        let delete_row = changes
            .iter()
            .find(|c| c.kind == ChangeKind::Delete)
            .expect("unclaimed old still deletes");
        assert_eq!(delete_row.path, "bbb");
    }

    #[test]
    fn rename_delete_skip() {
        let first = tree(vec![te("old", "h"), te("gone", "hx")]);
        let merged = tree(vec![te("p", "h")]);
        let prov_tree = tree(vec![te("p", "h")]);
        let prov: SideManifest = [("old".to_string(), "p".to_string())].into_iter().collect();
        let (changes, _) = manifest_diff(&first, &merged, &prov, &prov_tree);
        assert!(
            changes
                .iter()
                .all(|c| !(c.kind == ChangeKind::Delete && c.path == "old")),
            "emitted rename's old never also deletes: {changes:?}"
        );
        assert!(
            changes
                .iter()
                .any(|c| c.kind == ChangeKind::Delete && c.path == "gone"),
            "unclaimed deletes still emit"
        );
    }

    // ---------------------------------------------------------------
    // rebase
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn rebase_root() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let src = cs(&r, &[], "root-src", vec![add("f", &h)]).await;
        let np = cs(&r, &[], "root-target", vec![add("g", &h)]).await;
        let id = merged_id(
            r.engine
                .rebase(&r.repo, &src, &np, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        assert_eq!(
            rec.parents.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            vec![np.as_str()]
        );
        assert_eq!(rec.manifest.len(), 1);
        assert_eq!(rec.manifest[0].path, "f");
        assert_eq!(rec.manifest[0].kind, ChangeKind::Add);
    }

    #[tokio::test]
    async fn rebase_noop() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &h)]).await;
        let cs_id = cs(&r, &[&base], "cs", vec![add("g", &h)]).await;
        let before = r.db.vcs_count_changesets(&r.repo).await.unwrap();
        let out = r
            .engine
            .rebase(&r.repo, &cs_id, &base, "T", "t@example.com", None)
            .await
            .unwrap();
        assert_eq!(ff_head(out), cs_id);
        assert_eq!(r.db.vcs_count_changesets(&r.repo).await.unwrap(), before);
    }

    #[tokio::test]
    async fn rebase_descendant_cycle() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let c1 = cs(&r, &[], "c1", vec![add("f", &h)]).await;
        let c2 = cs(&r, &[&c1], "c2", vec![add("g", &h)]).await;
        let res = r
            .engine
            .rebase(&r.repo, &c1, &c2, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::InvalidInput(_))), "{res:?}");
    }

    #[tokio::test]
    async fn rebase_ancestor_rollback() {
        // Rebasing onto an ancestor is legal: replay applies only the
        // source's own delta (b: 0 → 9); the skipped ancestor's change to
        // `a` rolls back implicitly (target's value wins by t == b).
        let r = rig(MergeCaps::default()).await;
        let h1 = blob(&r, b"1\n").await;
        let h2 = blob(&r, b"2\n").await;
        let h0 = blob(&r, b"0\n").await;
        let h9 = blob(&r, b"9\n").await;
        let root = cs(&r, &[], "root", vec![add("a", &h1), add("b", &h0)]).await;
        let c1 = cs(&r, &[&root], "c1", vec![modify("a", &h2)]).await;
        let c2 = cs(&r, &[&c1], "c2", vec![modify("b", &h9)]).await;
        let id = merged_id(
            r.engine
                .rebase(&r.repo, &c2, &root, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        assert_eq!(
            rec.parents.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            vec![root.as_str()]
        );
        assert_eq!(rec.manifest.len(), 1);
        assert_eq!(rec.manifest[0].path, "b");
        assert_eq!(rec.manifest[0].kind, ChangeKind::Modify);
        assert_eq!(rec.manifest[0].blob_hash.as_deref(), Some(h9.as_str()));
    }

    #[tokio::test]
    async fn rebase_disjoint_target() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let h2 = blob(&r, b"y\n").await;
        let r1 = cs(&r, &[], "r1", vec![add("a", &h)]).await;
        let src = cs(&r, &[&r1], "src", vec![add("b", &h2)]).await;
        let r2 = cs(&r, &[], "r2", vec![add("x", &h)]).await;
        let id = merged_id(
            r.engine
                .rebase(&r.repo, &src, &r2, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        assert_eq!(rec.manifest.len(), 1);
        assert_eq!(rec.manifest[0].path, "b");
        assert_eq!(rec.manifest[0].kind, ChangeKind::Add);
    }

    #[tokio::test]
    async fn rebase_merge_node() {
        let r = rig(MergeCaps::default()).await;
        let h1 = blob(&r, b"1\n").await;
        let h2 = blob(&r, b"2\n").await;
        let hx = blob(&r, b"x\n").await;
        let root = cs(&r, &[], "root", vec![add("a", &h1)]).await;
        let p1 = cs(&r, &[&root], "p1", vec![modify("a", &h2)]).await;
        let p2 = cs(&r, &[&root], "p2", vec![add("x", &hx)]).await;
        // A merge-node changeset: parents [p1, p2], manifest vs p1.
        let m = cs(&r, &[&p1, &p2], "m", vec![add("x", &hx)]).await;
        // Replay m onto root — only its delta vs parents[0] (add x) applies.
        let id = merged_id(
            r.engine
                .rebase(&r.repo, &m, &root, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        assert_eq!(
            rec.parents.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
            vec![root.as_str()],
            "rebased output has exactly one parent"
        );
        assert_eq!(rec.manifest.len(), 1);
        assert_eq!(rec.manifest[0].path, "x");
        assert_eq!(rec.manifest[0].kind, ChangeKind::Add);
    }

    #[tokio::test]
    async fn rebase_message_default() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let h2 = blob(&r, b"y\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &h)]).await;
        let src = cs(&r, &[&base], "src message", vec![modify("f", &h2)]).await;
        let np = cs(&r, &[&base], "np", vec![add("g", &h)]).await;
        let id = merged_id(
            r.engine
                .rebase(&r.repo, &src, &np, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        assert_eq!(
            rec.message, "src message",
            "default replays source verbatim"
        );
    }

    #[tokio::test]
    async fn rebase_provenance_source_only() {
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let h2 = blob(&r, b"y\n").await;
        let base = cs(&r, &[], "base", vec![add("old", &h), add("keep", &h)]).await;
        // Target side: touches `keep` only — `old` stays live on target.
        let np = cs(&r, &[&base], "np", vec![modify("keep", &h2)]).await;
        // Source: rename old → p (keep inherited unchanged).
        let src = cs(&r, &[&base], "src", vec![rename("old", "p", &h)]).await;
        let id = merged_id(
            r.engine
                .rebase(&r.repo, &src, &np, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        let renames: Vec<_> = rec
            .manifest
            .iter()
            .filter(|c| c.kind == ChangeKind::Rename)
            .collect();
        assert_eq!(renames.len(), 1, "source manifest provenance emits rename");
        assert_eq!(renames[0].old_path.as_deref(), Some("old"));
        assert_eq!(renames[0].path, "p");
    }

    // ---------------------------------------------------------------
    // merge3 + paging + caps
    // ---------------------------------------------------------------

    #[test]
    fn merge3_trailing_newline() {
        // Trailing-newline-only divergence + another last-line edit ⇒
        // content conflict (last-line (bytes, ends_nl) pair differs).
        let res = merge3(b"x\ny\n".to_vec(), b"x\ny".to_vec(), b"x\nY\n".to_vec());
        assert!(matches!(res, Err(Merge3Fail::Regions(_))), "{res:?}");
        // Single-sided newline strip merges clean.
        let clean = merge3(b"x\ny\n".to_vec(), b"x\ny".to_vec(), b"x\ny\n".to_vec());
        assert_eq!(clean.unwrap(), b"x\ny");
    }

    #[test]
    fn merge3_crlf() {
        let out = merge3(
            b"a\r\nb\r\nc\r\n".to_vec(),
            b"a\r\nX\r\nc\r\n".to_vec(),
            b"a\r\nb\r\nY\r\n".to_vec(),
        )
        .unwrap();
        assert_eq!(out, b"a\r\nX\r\nY\r\n", "CRLF preserved verbatim");
    }

    #[test]
    fn merge3_terminators() {
        // No trailing newline: edits to both lines merge without one.
        let out = merge3(b"a\nb".to_vec(), b"A\nb".to_vec(), b"a\nB".to_vec()).unwrap();
        assert_eq!(out, b"A\nB");
        // Same shape with terminators keeps them.
        let out2 = merge3(b"a\nb\n".to_vec(), b"A\nb\n".to_vec(), b"a\nB\n".to_vec()).unwrap();
        assert_eq!(out2, b"A\nB\n");
    }

    #[test]
    fn merge3_line_cap() {
        let over_base = vec![b'\n'; 150_000];
        let over_side = vec![b'\n'; 60_000];
        // 150_000 + 60_000 + 2 > 200_000 → TooLarge on the ours pair.
        let res = merge3(over_base.clone(), over_side, Vec::new());
        assert!(matches!(res, Err(Merge3Fail::TooLarge)), "{res:?}");
        // At the cap proceeds (equal inputs merge clean).
        let at = vec![b'\n'; 99_999];
        let ok = merge3(at.clone(), at.clone(), Vec::new());
        assert!(ok.is_ok(), "n_base + n_side + 2 == LINE_CAP passes: {ok:?}");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn merge3_spawn_blocking_probe() {
        // The oneshot fires only if the runtime gets a poll slot while the
        // merge3 handle is awaited — impossible if the work ran inline.
        let (tx, mut rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = tx.send(());
        });
        let base: Vec<u8> = (0..50_000)
            .flat_map(|i| format!("line{i}\n").into_bytes())
            .collect();
        let mut ours = base.clone();
        ours[4] = b'X';
        let mut theirs = base.clone();
        theirs[100] = b'Y';
        let handle = run_merge3(base, ours, theirs);
        let out = handle.await.unwrap();
        assert!(rx.try_recv().is_ok(), "probe ran while awaiting merge3");
        assert!(out.is_ok());
    }

    #[tokio::test]
    async fn full_tree_paging() {
        // 600-entry tree exercises the tree() cursor loop (page = 500).
        let r = rig(MergeCaps::default()).await;
        r.db.vcs_insert_object_if_absent(&"aa".repeat(32), 1)
            .await
            .unwrap();
        let shared = "aa".repeat(32);
        let mut changes = Vec::new();
        for i in 0..600 {
            changes.push(add(&format!("f/{i:04}",), &shared));
        }
        let base = cs(&r, &[], "base", changes).await;
        let h = blob(&r, b"x\n").await;
        let ours = cs(&r, &[&base], "ours", vec![add("ours-only", &h)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![add("theirs-only", &h)]).await;
        let id = merged_id(
            r.engine
                .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
                .await
                .unwrap(),
        );
        let rec = r.store.get(&r.repo, &id).await.unwrap();
        // If full_tree truncated at 500 the missing entries would mint
        // phantom deletes — the manifest must be exactly the new file.
        assert_eq!(rec.manifest.len(), 1);
        assert_eq!(rec.manifest[0].path, "theirs-only");
        assert_eq!(rec.manifest[0].kind, ChangeKind::Add);
    }

    #[tokio::test]
    async fn cap_blob() {
        let caps = MergeCaps {
            merge_blob_max: 64,
            ..MergeCaps::default()
        };
        let r = rig(caps).await;
        let hb = blob(&r, b"base\n").await;
        let ho = blob(&r, &[b'o'; 100]).await;
        let ht = blob(&r, &[b't'; 60]).await;
        let base = cs(&r, &[], "base", vec![add("f", &hb)]).await;
        let ours = cs(&r, &[&base], "ours", vec![modify("f", &ho)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![modify("f", &ht)]).await;
        let res = r
            .engine
            .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::TooLarge)), "{res:?}");
    }

    #[tokio::test]
    async fn cap_paths_walk() {
        let caps = MergeCaps {
            merge_paths_max: 2,
            ..MergeCaps::default()
        };
        let r = rig(caps).await;
        let h = blob(&r, b"x\n").await;
        let base = cs(
            &r,
            &[],
            "base",
            vec![add("a", &h), add("b", &h), add("c", &h)],
        )
        .await;
        let ours = cs(&r, &[&base], "ours", vec![]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![]).await;
        let res = r
            .engine
            .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::TooLarge)), "{res:?}");
    }

    #[tokio::test]
    async fn cap_paths_manifest() {
        let caps = MergeCaps {
            merge_paths_max: 2,
            ..MergeCaps::default()
        };
        let r = rig(caps).await;
        let h = blob(&r, b"x\n").await;
        let h2 = blob(&r, b"y\n").await;
        // Trees stay ≤2 paths each but the emitted manifest has 3 rows.
        let base = cs(&r, &[], "base", vec![add("a", &h), add("b", &h)]).await;
        let ours = cs(&r, &[&base], "ours", vec![]).await;
        let theirs = cs(
            &r,
            &[&base],
            "theirs",
            vec![modify("a", &h2), del("b"), add("c", &h)],
        )
        .await;
        let res = r
            .engine
            .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::TooLarge)), "{res:?}");
    }

    #[tokio::test]
    async fn cap_ancestor_probe() {
        let caps = MergeCaps {
            ancestor_probe_max: 2,
            ..MergeCaps::default()
        };
        let r = rig(caps).await;
        let h = blob(&r, b"x\n").await;
        let c0 = cs(&r, &[], "c0", vec![add("f", &h)]).await;
        let c1 = cs(&r, &[&c0], "c1", vec![]).await;
        let c2 = cs(&r, &[&c1], "c2", vec![]).await;
        let c3 = cs(&r, &[&c2], "c3", vec![]).await;
        let x = cs(&r, &[], "x", vec![add("g", &h)]).await;
        // BFS from c3 visits {c3, c2, c1} = 3 > 2 → TooLarge.
        let res = r
            .engine
            .merge(&r.repo, &c3, &x, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::TooLarge)), "{res:?}");
    }

    #[tokio::test]
    async fn cap_side_walk() {
        let caps = MergeCaps {
            side_walk_max: 1,
            ..MergeCaps::default()
        };
        let r = rig(caps).await;
        let h = blob(&r, b"x\n").await;
        let base = cs(&r, &[], "base", vec![add("f", &h)]).await;
        let o1 = cs(&r, &[&base], "o1", vec![add("o1f", &h)]).await;
        let o2 = cs(&r, &[&o1], "o2", vec![add("o2f", &h)]).await;
        let t1 = cs(&r, &[&base], "t1", vec![add("t1f", &h)]).await;
        // ours' side-exclusive walk covers {o2, o1} = 2 > 1 → TooLarge.
        let res = r
            .engine
            .merge(&r.repo, &o2, &t1, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::TooLarge)), "{res:?}");
    }

    #[tokio::test]
    async fn cap_rename_probe() {
        let caps = MergeCaps {
            rename_probe_max: 0,
            ..MergeCaps::default()
        };
        let r = rig(caps).await;
        let h = blob(&r, b"x\n").await;
        let base = cs(&r, &[], "base", vec![add("old", &h), add("other", &h)]).await;
        let ours = cs(&r, &[&base], "ours", vec![rename("old", "y", &h)]).await;
        let theirs = cs(&r, &[&base], "theirs", vec![add("t", &h)]).await;
        let res = r
            .engine
            .merge(&r.repo, &ours, &theirs, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::TooLarge)), "{res:?}");
    }

    /// Build an engine over stores with a bespoke `max_object_bytes` so
    /// `MergeCaps::validate` can be exercised against it.
    fn mk_engine(
        db: Database,
        odir: &std::path::Path,
        max_object_bytes: u64,
        caps: MergeCaps,
    ) -> Result<MergeEngine, MergeError> {
        let c = ObjectStoreConfig {
            max_object_bytes,
            ..cfg(odir)
        };
        let objects = store_unchecked(db.clone(), c.clone());
        let index = ReadIndex::new(
            db.clone(),
            store_unchecked(db.clone(), c),
            ReadIndexCaps::default(),
        );
        MergeEngine::new(db.clone(), index, ChangesetStore::new(db), objects, caps)
    }

    #[tokio::test]
    async fn caps_validate_blob_vs_object() {
        let (_dbdir, db) = test_db().await;
        let odir = tempfile::tempdir().unwrap();
        let caps = MergeCaps {
            merge_blob_max: 128,
            ..MergeCaps::default()
        };
        let err = mk_engine(db, odir.path(), 64, caps).err();
        assert!(
            matches!(err, Some(MergeError::InvalidInput(_))),
            "merge_blob_max > max_object_bytes: {err:?}"
        );
    }

    #[tokio::test]
    async fn caps_validate_u64_max() {
        let (_dbdir, db) = test_db().await;
        let odir = tempfile::tempdir().unwrap();
        // max_object_bytes = u64::MAX lets the first check pass so the
        // dedicated `take(max + 1)` overflow guard is the tripping arm.
        let caps = MergeCaps {
            merge_blob_max: u64::MAX,
            ..MergeCaps::default()
        };
        let err = mk_engine(db, odir.path(), u64::MAX, caps).err();
        assert!(
            matches!(err, Some(MergeError::InvalidInput(_))),
            "merge_blob_max == u64::MAX: {err:?}"
        );
    }

    #[tokio::test]
    async fn caps_validate_paths_parity() {
        let (_dbdir, db) = test_db().await;
        let odir = tempfile::tempdir().unwrap();
        let caps = MergeCaps {
            merge_paths_max: 10_001,
            ..MergeCaps::default()
        };
        let err = mk_engine(db, odir.path(), u64::MAX, caps).err();
        assert!(
            matches!(err, Some(MergeError::InvalidInput(_))),
            "merge_paths_max above the wire bound: {err:?}"
        );
    }

    // ---------------------------------------------------------------
    // error bridge (Q8)
    // ---------------------------------------------------------------

    #[test]
    fn bridge_cursor_malformed() {
        // Engine-minted cursors are never caller input — malformed means
        // index corruption ⇒ Internal, not InvalidInput.
        match map_index_err(IndexError::CursorMalformed) {
            MergeError::Internal(_) => {}
            other => panic!("expected Internal: {other:?}"),
        }
    }

    #[test]
    fn bridge_frontier() {
        match map_index_err(IndexError::FrontierTooLarge) {
            MergeError::TooLarge => {}
            other => panic!("expected TooLarge: {other:?}"),
        }
    }

    #[tokio::test]
    async fn bridge_base_not_found() {
        // Unit arm…
        match map_index_err(IndexError::BaseNotFound) {
            MergeError::NoCommonAncestor => {}
            other => panic!("expected NoCommonAncestor: {other:?}"),
        }
        // …and the live path — disjoint histories surface it end-to-end.
        let r = rig(MergeCaps::default()).await;
        let h = blob(&r, b"x\n").await;
        let a = cs(&r, &[], "a", vec![add("f", &h)]).await;
        let b = cs(&r, &[], "b", vec![add("g", &h)]).await;
        let res = r
            .engine
            .merge(&r.repo, &a, &b, "T", "t@example.com", None)
            .await;
        assert!(matches!(res, Err(MergeError::NoCommonAncestor)), "{res:?}");
    }

    #[test]
    fn bridge_parent_not_found() {
        for e in [
            ChangesetError::ParentNotFound,
            ChangesetError::ChangesetNotFound,
        ] {
            match map_changeset_err(e) {
                MergeError::NotFound => {}
                other => panic!("expected NotFound: {other:?}"),
            }
        }
    }

    #[test]
    fn bridge_object_not_found() {
        // A stored manifest's blob gone — integrity loss ⇒ Internal.
        match map_changeset_err(ChangesetError::ObjectNotFound { hash: None }) {
            MergeError::Internal(_) => {}
            other => panic!("expected Internal: {other:?}"),
        }
    }

    #[test]
    fn bridge_quota_scope() {
        match map_object_err(ObjectStoreError::QuotaExceeded(QuotaScope::Object)) {
            MergeError::QuotaExceeded(QuotaScope::Object) => {}
            other => panic!("expected QuotaExceeded(Object): {other:?}"),
        }
        match map_object_err(ObjectStoreError::QuotaExceeded(QuotaScope::Repo)) {
            MergeError::QuotaExceeded(QuotaScope::Repo) => {}
            other => panic!("expected QuotaExceeded(Repo): {other:?}"),
        }
    }

    #[tokio::test]
    async fn bridge_db_not_configured() {
        let odir = tempfile::tempdir().unwrap();
        let skipped = Database::skipped();
        let c = cfg(odir.path());
        let objects = ObjectStore::new(skipped.clone(), c.clone()).unwrap();
        let index = ReadIndex::new(
            skipped.clone(),
            store_unchecked(skipped.clone(), c),
            ReadIndexCaps::default(),
        );
        let engine = MergeEngine::new(
            skipped.clone(),
            index,
            ChangesetStore::new(skipped),
            objects,
            MergeCaps::default(),
        )
        .unwrap();
        // is_skipped probes FIRST — even malformed ids get DbNotConfigured.
        let res = engine.merge("r", "junk", "junk", "T", "t@e", None).await;
        assert!(matches!(res, Err(MergeError::DbNotConfigured)), "{res:?}");
        let res = engine.rebase("r", "junk", "junk", "T", "t@e", None).await;
        assert!(matches!(res, Err(MergeError::DbNotConfigured)), "{res:?}");
    }

    #[tokio::test]
    async fn malformed_id_no_echo() {
        let r = rig(MergeCaps::default()).await;
        let res = r
            .engine
            .merge(
                &r.repo,
                "bad-id!!",
                "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR",
                "T",
                "t@e",
                None,
            )
            .await;
        let err = res.unwrap_err();
        assert!(matches!(err, MergeError::InvalidInput(_)), "{err:?}");
        let text = err.to_string();
        assert_eq!(text, "invalid input: changeset id: malformed");
        assert!(
            !text.contains("bad-id"),
            "parser text must not echo: {text}"
        );
    }

    #[test]
    fn conflict_display_count_only() {
        let e = MergeError::Conflict(vec![
            entry("secret/path", ConflictKind::Content, None),
            entry("other/secret", ConflictKind::Binary, None),
        ]);
        let text = e.to_string();
        assert_eq!(text, "conflict: 2 paths");
        assert!(!text.contains("secret"), "paths never appear: {text}");
    }
}
