//! Derived-index read model (u4) — the walk side of the VCS core.
//!
//! Writes: [`ReadIndex::update_for`] builds the `path_touch` rows folded into
//! the changeset-create transaction; tree snapshots (`tree_entry` +
//! `tree_snap` written last, so a crash mid-materialize resumes cleanly —
//! entries without a snap are just recomputed), `merge_base` cache rows, and
//! `blame` shards (≤32KB payload each) upsert lazily on first read.
//!
//! Reads: `log` (best-first heap over `(committed_at, id)` with a
//! skip-replay `{"n"}` cursor), `merge_base` (mark `lo` ancestry, level-BFS
//! from `hi`, canonical `(lo,hi)` cache key), `diff` (BTreeSet-ordered path
//! candidates), `tree`/`file_at` (first-parent delta fold over snapshots),
//! `path_history` (chronological `path_touch` block scan), `blame`
//! (ancestry-ordered touch chain through [`crate::text_diff::diff_lines`]).
//!
//! Cursors are base64url-no-pad JSON, ≤1KB; malformed/wrong-shape decodes
//! are [`IndexError::CursorMalformed`]. Every changeset input is
//! existence-checked before traversal (U4-AR-02) — `InvalidInput` on
//! malformed ids, `NotFound` on absent rows. Hashed-path reads always
//! re-check `payload.path == arg` — keys are the mechanism, payloads are the
//! proof (SR4.10).

use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque};

use base64::Engine;
use oxidean_core::vcs_types::ChangesetId;
use oxidean_db::{Database, VcsDerivedIndexRow, VcsDerivedIndexUpsert};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

use crate::changeset_store::{
    is_changeset_id, is_git_oid, validate_path, ChangeKind, ChangesetError, ChangesetRecord,
    ChangesetStore, FileChangeInput,
};
use crate::object_store::{ObjectStore, ObjectStoreError};
use crate::ref_registry::{RefError, RefRegistry};
use crate::text_diff::diff_lines;

const KIND_PATH_TOUCH: &str = "path_touch";
const KIND_TREE_SNAP: &str = "tree_snap";
const KIND_TREE_ENTRY: &str = "tree_entry";
const KIND_MERGE_BASE: &str = "merge_base";
const KIND_BLAME: &str = "blame";
// Keys embed `\x01` separators inline (e.g. `{hash}\x01{ts}\x01{cs}`) — a
// validated path can never contain a control byte, so segments never
// collide with path content.
const SCAN_PAGE: i64 = 500;
const CURSOR_MAX: usize = 1024;
/// Blame shard payload ceiling (SR4.8 — well under the 64KB MySQL TEXT
/// ceiling so LWW upserts never silently truncate).
const BLAME_SHARD_MAX: usize = 32 * 1024;

/// Errors surfaced by every read walk. u7 maps variants to wire codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexError {
    /// A changeset input does not exist.
    NotFound,
    /// `file_at`/`blame` on a path absent from the tree at that changeset.
    PathNotFound,
    /// `merge_base`/`diff` on disjoint histories (the Q4 `NoCommonAncestor`
    /// name is aliased here).
    BaseNotFound,
    /// A bounded walk tripped its named cap (`seen`/`mark`/`diff
    /// candidates`/`blame touches`) — the honest-bounds tripwire (SR4.8).
    FrontierTooLarge,
    /// Cursor did not decode to the walk's expected shape.
    CursorMalformed,
    /// Malformed argument (id shape, path grammar, limit) — `field: reason`
    /// text; never echoes the rejected value.
    InvalidInput(String),
    /// Raw facade/io detail for host diagnostics — non-sentinel DB errors,
    /// store inconsistencies, and `ObjectStore::get` I/O failures all land
    /// here (opaque to the wire, LC4.6).
    Internal(String),
    /// `Database::is_skipped()` — eager fail-closed check.
    DbNotConfigured,
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IndexError::NotFound => write!(f, "not found"),
            IndexError::PathNotFound => write!(f, "path not found"),
            IndexError::BaseNotFound => write!(f, "no common ancestor"),
            IndexError::FrontierTooLarge => write!(f, "walk frontier too large"),
            IndexError::CursorMalformed => write!(f, "malformed cursor"),
            IndexError::InvalidInput(reason) => write!(f, "invalid input: {reason}"),
            IndexError::Internal(detail) => write!(f, "internal error: {detail}"),
            IndexError::DbNotConfigured => write!(f, "database not configured"),
        }
    }
}

impl std::error::Error for IndexError {}

/// Every typed-error return warns once at construction — `error = %e`
/// carries the typed error only; ids/hashes/counts may ride as their own
/// fields, never path/message/author values (gc.rs precedent).
fn warned(e: IndexError) -> IndexError {
    tracing::warn!(error = %e, "vcs index typed error");
    e
}

fn internal(detail: String) -> IndexError {
    warned(IndexError::Internal(detail))
}

fn invalid(field: &str, reason: &str) -> IndexError {
    warned(IndexError::InvalidInput(format!("{field}: {reason}")))
}

fn frontier() -> IndexError {
    warned(IndexError::FrontierTooLarge)
}

fn malformed_cursor() -> IndexError {
    warned(IndexError::CursorMalformed)
}

fn map_changeset_err(e: ChangesetError) -> IndexError {
    match e {
        ChangesetError::ChangesetNotFound => warned(IndexError::NotFound),
        ChangesetError::InvalidInput(reason) => warned(IndexError::InvalidInput(reason)),
        ChangesetError::DbNotConfigured => warned(IndexError::DbNotConfigured),
        // ParentNotFound/ObjectNotFound degrade to opaque detail here —
        // the changeset store already warned at its own origin.
        other => internal(other.to_string()),
    }
}

fn map_ref_err(e: RefError) -> IndexError {
    match e {
        RefError::RefNotFound => warned(IndexError::NotFound),
        RefError::InvalidInput(reason) => warned(IndexError::InvalidInput(reason)),
        RefError::DbNotConfigured => warned(IndexError::DbNotConfigured),
        other => internal(other.to_string()),
    }
}

/// Injectable walk caps (SR4.8) — tests override them to exercise
/// `FrontierTooLarge` without 250k-node fixtures.
#[derive(Debug, Clone)]
pub struct ReadIndexCaps {
    /// `log` dedup set (`pushed ∪ emitted ∪ frontier`).
    pub seen_max: usize,
    /// `merge_base` mark set / `blame` ancestry map.
    pub mark_max: usize,
    /// `diff` candidate-walk visited set.
    pub diff_candidates_max: usize,
    /// `blame` touch-collection bound (touches + rename-chain hops).
    pub blame_touches_max: usize,
}

impl Default for ReadIndexCaps {
    fn default() -> Self {
        Self {
            seen_max: 65_536,
            mark_max: 250_000,
            diff_candidates_max: 250_000,
            blame_touches_max: 65_536,
        }
    }
}

/// `log` entry — the full stored record (manifest included).
pub type LogEntry = ChangesetRecord;

/// Kind of tree-level difference a `diff` entry reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    Add,
    Delete,
    Modify,
}

impl DiffKind {
    pub fn as_str(self) -> &'static str {
        match self {
            DiffKind::Add => "add",
            DiffKind::Delete => "delete",
            DiffKind::Modify => "modify",
        }
    }
}

/// One path-level delta between two changesets (`DiffKind`, not the
/// manifest `ChangeKind` — renames surface as delete+add).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    pub path: String,
    pub kind: DiffKind,
    pub old_mode: Option<i64>,
    pub new_mode: Option<i64>,
    pub old_blob: Option<String>,
    pub new_blob: Option<String>,
}

/// A materialized tree entry — `path` is relative to the scanned tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    pub mode: i64,
    pub blob_hash: String,
}

/// `file_at` result — the caller already knows the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub mode: i64,
    pub blob_hash: String,
}

/// One decoded `path_touch` row (rename rows included — the rename-away
/// `\x01r` rows surface with `old_path == path` and `kind == Rename`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTouch {
    pub path: String,
    pub kind: ChangeKind,
    pub changeset_id: ChangesetId,
    pub committed_at: String,
    pub old_path: Option<String>,
    pub blob: Option<String>,
    pub mode: i64,
}

/// One blamed line — `line_no` is 1-based against the blob at `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub line_no: u64,
    pub changeset_id: ChangesetId,
    pub author: String,
    pub content: Vec<u8>,
}

/// Owned facade-input material for `path_touch` rows — the create path
/// borrows `&str`s off these owned strings (borrowed facade inserts cannot
/// reference freshly allocated keys/payloads any other way).
pub(crate) struct PathTouchRow {
    pub(crate) kind: &'static str,
    pub(crate) index_key: String,
    pub(crate) payload: String,
    pub(crate) covering_changeset: String,
}

// ---------------------------------------------------------------------------
// Payload / cursor wire shapes (serde field order is contractual).
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct PathTouchPayloadRef<'a> {
    path: &'a str,
    kind: &'a str,
    changeset_id: &'a str,
    committed_at: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob: Option<&'a str>,
    mode: i64,
}

#[derive(Deserialize)]
struct PathTouchPayload {
    path: String,
    kind: String,
    changeset_id: String,
    committed_at: String,
    old_path: Option<String>,
    blob: Option<String>,
    #[allow(dead_code)]
    mode: i64,
}

#[derive(Serialize, Deserialize)]
struct TreeEntryPayload {
    mode: i64,
    blob: String,
}

#[derive(Serialize)]
struct TreeSnapPayload {
    written_at: String,
}

#[derive(Serialize, Deserialize)]
struct MergeBasePayload {
    base: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct BlameShardEntry {
    /// 1-based line number.
    n: u64,
    /// Introducing changeset id.
    c: String,
    /// Composed author string.
    a: String,
    /// base64url-no-pad line bytes.
    b: String,
}

#[derive(Serialize, Deserialize)]
struct BlameShard {
    path: String,
    lines: u64,
    entries: Vec<BlameShardEntry>,
}

#[derive(Serialize, Deserialize)]
struct NCursor {
    n: u64,
}

#[derive(Serialize, Deserialize)]
struct KCursor {
    k: String,
}

#[derive(Serialize, Deserialize)]
struct BlameCursor {
    k: String,
    off: u64,
}

fn encode_cursor<T: Serialize>(v: &T) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(v).unwrap_or_default())
}

fn decode_cursor<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, IndexError> {
    if raw.len() > CURSOR_MAX {
        return Err(malformed_cursor());
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.as_bytes())
        .map_err(|_| malformed_cursor())?;
    if bytes.len() > CURSOR_MAX {
        return Err(malformed_cursor());
    }
    serde_json::from_slice(&bytes).map_err(|_| malformed_cursor())
}

/// First 32 hex chars of sha256(path) — the hashed-path key segment
/// (128-bit window; `payload.path == arg` guards it, SR4.10).
pub(crate) fn path_hash32(path: &str) -> String {
    hex::encode(&Sha256::digest(path.as_bytes())[..16])
}

/// `b'\n'` split preserving every other byte verbatim; a trailing empty
/// segment after a final newline is not a line. CRLF stays inside the line.
pub(crate) fn split_lines(bytes: &[u8]) -> Vec<Vec<u8>> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<Vec<u8>> = bytes.split(|b| *b == b'\n').map(|s| s.to_vec()).collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

fn validate_read_path(path: &str) -> Result<(), IndexError> {
    validate_path(path).map_err(|e| match e {
        ChangesetError::InvalidInput(reason) => warned(IndexError::InvalidInput(reason)),
        other => internal(other.to_string()),
    })
}

/// The derived-index read model.
pub struct ReadIndex {
    db: Database,
    objects: ObjectStore,
    caps: ReadIndexCaps,
}

impl ReadIndex {
    pub fn new(db: Database, objects: ObjectStore, caps: ReadIndexCaps) -> Self {
        Self { db, objects, caps }
    }

    // ------------------------------------------------------------------
    // Write side
    // ------------------------------------------------------------------

    /// Build the `path_touch` rows for a creating changeset (static —
    /// returns owned rows the caller borrows into `VcsDerivedIndexUpsert`).
    /// Each change lands one row under `{hash32(path)}\x01{committed_at}\x01
    /// {cs}`; a rename additionally lands its old-path row under
    /// `{hash32(old)}\x01{ts}\x01{cs}\x01r` — the `\x01r` discriminator keeps
    /// move-and-recreate (`{rename a→b} + {add a}`) keys distinct inside one
    /// changeset (SR4.6/AR-01).
    pub(crate) fn update_for(
        id: &str,
        committed_at: &str,
        changes: &[FileChangeInput],
    ) -> Vec<PathTouchRow> {
        let mut rows = Vec::with_capacity(changes.len() * 2);
        for c in changes {
            let payload = serde_json::to_string(&PathTouchPayloadRef {
                path: &c.path,
                kind: c.kind.as_str(),
                changeset_id: id,
                committed_at,
                old_path: c.old_path.as_deref(),
                blob: c.blob_hash.as_deref(),
                mode: c.mode,
            })
            .unwrap_or_default();
            rows.push(PathTouchRow {
                kind: KIND_PATH_TOUCH,
                index_key: format!("{}\x01{}\x01{}", path_hash32(&c.path), committed_at, id),
                payload,
                covering_changeset: id.to_string(),
            });
            if c.kind == ChangeKind::Rename {
                // `old_path` presence is validated upstream; skip the r-row
                // defensively if a caller bypasses validation.
                if let Some(old) = c.old_path.as_deref() {
                    let payload_r = serde_json::to_string(&PathTouchPayloadRef {
                        path: old,
                        kind: ChangeKind::Rename.as_str(),
                        changeset_id: id,
                        committed_at,
                        old_path: Some(old),
                        blob: c.blob_hash.as_deref(),
                        mode: c.mode,
                    })
                    .unwrap_or_default();
                    rows.push(PathTouchRow {
                        kind: KIND_PATH_TOUCH,
                        index_key: format!(
                            "{}\x01{}\x01{}\x01r",
                            path_hash32(old),
                            committed_at,
                            id
                        ),
                        payload: payload_r,
                        covering_changeset: id.to_string(),
                    });
                }
            }
        }
        rows
    }

    // ------------------------------------------------------------------
    // Facade wrappers
    // ------------------------------------------------------------------

    async fn index_probe(
        &self,
        repo_id: &str,
        kind: &str,
        key: &str,
    ) -> Result<Option<VcsDerivedIndexRow>, IndexError> {
        self.db
            .vcs_query_derived_index(repo_id, kind, Some(key), None, 1)
            .await
            .map(|mut v| v.pop())
            .map_err(internal)
    }

    async fn index_page(
        &self,
        repo_id: &str,
        kind: &str,
        cursor: &str,
    ) -> Result<Vec<VcsDerivedIndexRow>, IndexError> {
        self.db
            .vcs_query_derived_index(repo_id, kind, None, Some(cursor), SCAN_PAGE)
            .await
            .map_err(internal)
    }

    async fn upsert_rows(
        &self,
        repo_id: &str,
        rows: &[VcsDerivedIndexUpsert<'_>],
    ) -> Result<(), IndexError> {
        for chunk in rows.chunks(SCAN_PAGE as usize) {
            self.db
                .vcs_upsert_derived_index_rows(repo_id, chunk)
                .await
                .map_err(internal)?;
        }
        tracing::info!(
            event = "vcs.index.upsert",
            rows = rows.len(),
            "derived-index rows written"
        );
        Ok(())
    }

    /// U4-AR-02: every changeset input is existence-checked before any walk.
    async fn changeset_row(
        &self,
        repo_id: &str,
        id: &str,
    ) -> Result<oxidean_db::VcsChangesetRow, IndexError> {
        if !is_changeset_id(id) {
            return Err(invalid("id", "not a changeset id"));
        }
        self.db
            .vcs_get_changeset(repo_id, id)
            .await
            .map_err(internal)?
            .ok_or_else(|| warned(IndexError::NotFound))
    }

    async fn parents_of(&self, repo_id: &str, id: &str) -> Result<Vec<String>, IndexError> {
        self.db
            .vcs_changeset_parents(repo_id, id)
            .await
            .map_err(internal)
    }

    // ------------------------------------------------------------------
    // log
    // ------------------------------------------------------------------

    /// `from` resolves in precedence order: changeset id → git oid → ref
    /// name (id-precedence is why id-shaped ref names are rejected, BR4.9).
    /// Entries pop newest-first by `(committed_at, id)`; the `{"n"}` cursor
    /// is a deterministic skip-replay count — re-walking a stable graph
    /// replays identical pops, so no entry is emitted twice across pages.
    pub async fn log(
        &self,
        repo_id: &str,
        from: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<LogEntry>, Option<String>), IndexError> {
        if self.db.is_skipped() {
            return Err(warned(IndexError::DbNotConfigured));
        }
        let start_id: String = if is_changeset_id(from) {
            from.to_string()
        } else if is_git_oid(from) {
            match ChangesetStore::new(self.db.clone())
                .resolve_git_oid(repo_id, from)
                .await
                .map_err(map_changeset_err)?
            {
                Some(id) => id.as_str().to_string(),
                None => return Err(warned(IndexError::NotFound)),
            }
        } else {
            RefRegistry::new(self.db.clone())
                .resolve(repo_id, from)
                .await
                .map_err(map_ref_err)?
                .as_str()
                .to_string()
        };
        let start = self.changeset_row(repo_id, &start_id).await?;

        let skip = match cursor {
            None => 0u64,
            Some(raw) => decode_cursor::<NCursor>(raw)?.n,
        };
        let limit = limit.clamp(1, SCAN_PAGE as usize);

        // Max-heap pops the newest (committed_at, id) first.
        let mut heap: BinaryHeap<(String, String)> = BinaryHeap::new();
        let mut rows: HashMap<String, oxidean_db::VcsChangesetRow> = HashMap::new();
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(start_id.clone());
        rows.insert(start_id.clone(), start.clone());
        heap.push((start.committed_at.clone(), start_id));

        let mut popped: u64 = 0;
        let mut emitted: Vec<LogEntry> = Vec::new();
        while let Some((_, id)) = heap.pop() {
            popped += 1;
            if popped > skip && emitted.len() < limit {
                let row = rows
                    .get(&id)
                    .ok_or_else(|| internal("log walk lost a fetched row".to_string()))?;
                let manifest = self.manifest_of(&id).await?;
                let raw_parents: Vec<String> =
                    serde_json::from_str(&row.parents).map_err(|e| internal(e.to_string()))?;
                let parents = raw_parents
                    .iter()
                    .map(|p| ChangesetId::parse(p).map_err(internal))
                    .collect::<Result<Vec<_>, _>>()?;
                emitted.push(ChangesetRecord {
                    id: ChangesetId::parse(&row.id).map_err(internal)?,
                    repo_id: row.repo_id.clone(),
                    parents,
                    author: row.author.clone(),
                    message: row.message.clone(),
                    committed_at: row.committed_at.clone(),
                    manifest_hash: row.manifest_hash.clone(),
                    manifest,
                });
            }
            for parent in self.parents_of(repo_id, &id).await? {
                if seen.insert(parent.clone()) {
                    if seen.len() > self.caps.seen_max {
                        return Err(frontier());
                    }
                    let row = self
                        .db
                        .vcs_get_changeset(repo_id, &parent)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| internal("changeset vanished mid-walk".to_string()))?;
                    heap.push((row.committed_at.clone(), parent.clone()));
                    rows.insert(parent, row);
                }
            }
            if emitted.len() == limit && popped > skip {
                break;
            }
        }
        let next = if heap.is_empty() {
            None
        } else {
            Some(encode_cursor(&NCursor { n: popped }))
        };
        Ok((emitted, next))
    }

    async fn manifest_of(&self, id: &str) -> Result<Vec<FileChangeInput>, IndexError> {
        let rows = self.db.vcs_list_file_changes(id).await.map_err(internal)?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            out.push(FileChangeInput {
                path: r.path,
                kind: ChangeKind::parse(&r.kind)
                    .ok_or_else(|| internal(format!("stored file_change kind: {}", r.kind)))?,
                mode: r.mode,
                old_path: r.old_path,
                blob_hash: r.blob_hash,
            });
        }
        Ok(out)
    }

    // ------------------------------------------------------------------
    // merge_base
    // ------------------------------------------------------------------

    /// Deterministic single-answer merge base. Canonical `(lo, hi)`
    /// orientation makes `merge_base(a,b) == merge_base(b,a)`; the cache row
    /// `{lo}\x01{hi}` is written under `hi` coverage. May pick a dominated
    /// ancestor on multi-branch topologies (SR4.11 — deterministic and
    /// contract-adequate).
    pub async fn merge_base(
        &self,
        repo_id: &str,
        a: &str,
        b: &str,
    ) -> Result<ChangesetId, IndexError> {
        if self.db.is_skipped() {
            return Err(warned(IndexError::DbNotConfigured));
        }
        let ra = self.changeset_row(repo_id, a).await?;
        let rb = self.changeset_row(repo_id, b).await?;
        let (lo, hi) = if ra.id <= rb.id {
            (ra.id, rb.id)
        } else {
            (rb.id, ra.id)
        };

        let cache_key = format!("{lo}\x01{hi}");
        if let Some(row) = self
            .index_probe(repo_id, KIND_MERGE_BASE, &cache_key)
            .await?
        {
            let payload: MergeBasePayload =
                serde_json::from_str(&row.payload).map_err(|e| internal(e.to_string()))?;
            return ChangesetId::parse(&payload.base).map_err(internal);
        }

        // Mark lo AND all its ancestors (seed = {lo} — a fast-forward base
        // returns lo itself, SR4.11).
        let mut marked: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        marked.insert(lo.clone());
        queue.push_back(lo.clone());
        while let Some(x) = queue.pop_front() {
            for p in self.parents_of(repo_id, &x).await? {
                if marked.insert(p.clone()) {
                    if marked.len() > self.caps.mark_max {
                        return Err(frontier());
                    }
                    queue.push_back(p);
                }
            }
        }

        // Level-BFS from hi: first level containing a marked node wins;
        // within a level the newest (committed_at, id) candidate wins.
        let mut visited: HashSet<String> = HashSet::new();
        visited.insert(hi.clone());
        let mut level: Vec<String> = vec![hi.clone()];
        loop {
            let candidates: Vec<&String> = level.iter().filter(|x| marked.contains(*x)).collect();
            if !candidates.is_empty() {
                let mut best: Option<(String, String)> = None; // (committed_at, id)
                for c in candidates {
                    let row = self
                        .db
                        .vcs_get_changeset(repo_id, c)
                        .await
                        .map_err(internal)?
                        .ok_or_else(|| {
                            internal("merge-base candidate vanished mid-walk".to_string())
                        })?;
                    let cand = (row.committed_at.clone(), row.id.clone());
                    if best.as_ref().is_none_or(|b| cand > *b) {
                        best = Some(cand);
                    }
                }
                let (_, pick) =
                    best.ok_or_else(|| internal("merge-base level had no candidate".to_string()))?;
                let payload = serde_json::to_string(&MergeBasePayload { base: pick.clone() })
                    .unwrap_or_default();
                self.upsert_rows(
                    repo_id,
                    &[VcsDerivedIndexUpsert {
                        kind: KIND_MERGE_BASE,
                        index_key: &cache_key,
                        payload: &payload,
                        covering_changeset: &hi,
                    }],
                )
                .await?;
                return ChangesetId::parse(&pick).map_err(internal);
            }
            let mut next_level: Vec<String> = Vec::new();
            for x in &level {
                for p in self.parents_of(repo_id, x).await? {
                    if visited.insert(p.clone()) {
                        if visited.len() + marked.len() > self.caps.mark_max {
                            return Err(frontier());
                        }
                        next_level.push(p);
                    }
                }
            }
            if next_level.is_empty() {
                return Err(warned(IndexError::BaseNotFound));
            }
            level = next_level;
        }
    }

    // ------------------------------------------------------------------
    // tree / file_at / materialize
    // ------------------------------------------------------------------

    async fn tree_snap_exists(&self, repo_id: &str, cs: &str) -> Result<bool, IndexError> {
        Ok(self
            .index_probe(repo_id, KIND_TREE_SNAP, cs)
            .await?
            .is_some())
    }

    async fn tree_entry_probe(
        &self,
        repo_id: &str,
        cs: &str,
        path: &str,
    ) -> Result<Option<(i64, String)>, IndexError> {
        let key = format!("{cs}\x01{path}");
        let Some(row) = self.index_probe(repo_id, KIND_TREE_ENTRY, &key).await? else {
            return Ok(None);
        };
        let payload: TreeEntryPayload =
            serde_json::from_str(&row.payload).map_err(|e| internal(e.to_string()))?;
        Ok(Some((payload.mode, payload.blob)))
    }

    /// Full `{cs}\x01` block of `tree_entry` rows → path→(mode, blob).
    async fn load_tree_entries(
        &self,
        repo_id: &str,
        cs: &str,
    ) -> Result<BTreeMap<String, (i64, String)>, IndexError> {
        let block = format!("{cs}\x01");
        let mut map = BTreeMap::new();
        let mut cur = block.clone();
        loop {
            let rows = self.index_page(repo_id, KIND_TREE_ENTRY, &cur).await?;
            let fetched = rows.len();
            let mut done = false;
            for r in rows {
                if !r.index_key.starts_with(&block) {
                    done = true;
                    break;
                }
                cur = r.index_key.clone();
                let suffix = &r.index_key[block.len()..];
                let payload: TreeEntryPayload =
                    serde_json::from_str(&r.payload).map_err(|e| internal(e.to_string()))?;
                map.insert(suffix.to_string(), (payload.mode, payload.blob));
            }
            if done || fetched < SCAN_PAGE as usize {
                return Ok(map);
            }
        }
    }

    /// Materialize `tree(cs)` — fold manifests back along the first-parent
    /// chain to the nearest ancestor snapshot (or the empty root), then
    /// write `tree_entry` rows chunked and the `tree_snap` marker LAST so a
    /// crash mid-write leaves entries without a snap — the next read
    /// recomputes cleanly (crash-resume by construction).
    async fn materialize(
        &self,
        repo_id: &str,
        cs: &str,
    ) -> Result<BTreeMap<String, (i64, String)>, IndexError> {
        let mut chain = vec![cs.to_string()];
        let mut cur = cs.to_string();
        let mut map: BTreeMap<String, (i64, String)> = loop {
            let parents = self.parents_of(repo_id, &cur).await?;
            let Some(next) = parents.into_iter().next() else {
                break BTreeMap::new();
            };
            cur = next;
            if self.tree_snap_exists(repo_id, &cur).await? {
                break self.load_tree_entries(repo_id, &cur).await?;
            }
            chain.push(cur.clone());
        };
        for node in chain.iter().rev() {
            let fcs = self
                .db
                .vcs_list_file_changes(node)
                .await
                .map_err(internal)?;
            // Sequential per-entry move semantics in manifest (path) order:
            // each entry's removals land BEFORE its own write and BEFORE the
            // next entry — a chained rename {A→B, B→C} moves B away again
            // instead of leaving a phantom (R3-fix).
            for fc in &fcs {
                match fc.kind.as_str() {
                    "delete" => {
                        map.remove(&fc.path);
                    }
                    "rename" => {
                        let old = fc.old_path.as_deref().ok_or_else(|| {
                            internal("rename file_change row missing old_path".to_string())
                        })?;
                        map.remove(old);
                        let blob = fc.blob_hash.clone().ok_or_else(|| {
                            internal("non-delete file_change row missing blob".to_string())
                        })?;
                        map.insert(fc.path.clone(), (fc.mode, blob));
                    }
                    _ => {
                        let blob = fc.blob_hash.clone().ok_or_else(|| {
                            internal("non-delete file_change row missing blob".to_string())
                        })?;
                        map.insert(fc.path.clone(), (fc.mode, blob));
                    }
                }
            }
        }
        // Write entries, then the snap marker LAST.
        let entries: Vec<(String, String)> = map
            .iter()
            .map(|(p, (m, b))| {
                (
                    format!("{cs}\x01{p}"),
                    serde_json::to_string(&TreeEntryPayload {
                        mode: *m,
                        blob: b.clone(),
                    })
                    .unwrap_or_default(),
                )
            })
            .collect();
        for chunk in entries.chunks(SCAN_PAGE as usize) {
            let upserts: Vec<VcsDerivedIndexUpsert<'_>> = chunk
                .iter()
                .map(|(k, p)| VcsDerivedIndexUpsert {
                    kind: KIND_TREE_ENTRY,
                    index_key: k,
                    payload: p,
                    covering_changeset: cs,
                })
                .collect();
            self.db
                .vcs_upsert_derived_index_rows(repo_id, &upserts)
                .await
                .map_err(internal)?;
        }
        let snap = serde_json::to_string(&TreeSnapPayload {
            written_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })
        .unwrap_or_default();
        self.db
            .vcs_upsert_derived_index_rows(
                repo_id,
                &[VcsDerivedIndexUpsert {
                    kind: KIND_TREE_SNAP,
                    index_key: cs,
                    payload: &snap,
                    covering_changeset: cs,
                }],
            )
            .await
            .map_err(internal)?;
        tracing::info!(
            event = "vcs.index.tree_materialized",
            entries = map.len(),
            "tree snapshot written"
        );
        Ok(map)
    }

    /// `(mode, blob)` of `path` at `cs` — first-parent fold with snapshot
    /// short-circuit; `None` when the path is absent (delete covered it or
    /// it never existed).
    async fn file_state_at(
        &self,
        repo_id: &str,
        cs: &str,
        path: &str,
    ) -> Result<Option<(i64, String)>, IndexError> {
        let mut cur = cs.to_string();
        loop {
            if self.tree_snap_exists(repo_id, &cur).await? {
                return self.tree_entry_probe(repo_id, &cur, path).await;
            }
            let fcs = self
                .db
                .vcs_list_file_changes(&cur)
                .await
                .map_err(internal)?;
            // Same sequential semantics as materialize: the LAST manifest
            // toucher of `path` at this changeset wins — a write may be
            // cancelled by a later entry renaming `path` away (chained
            // rename), and a rename-away may be superseded by a later write
            // back to `path` (move-and-recreate).
            let mut state: Option<Option<(i64, String)>> = None;
            for fc in &fcs {
                if fc.kind == "rename" && fc.old_path.as_deref() == Some(path) {
                    state = Some(None);
                }
                if fc.path == path {
                    state = Some(if fc.kind == "delete" {
                        None
                    } else {
                        let blob = fc.blob_hash.clone().ok_or_else(|| {
                            internal("non-delete file_change row missing blob".to_string())
                        })?;
                        Some((fc.mode, blob))
                    });
                }
            }
            if let Some(state) = state {
                return Ok(state);
            }
            let parents = self.parents_of(repo_id, &cur).await?;
            let Some(next) = parents.into_iter().next() else {
                return Ok(None);
            };
            cur = next;
        }
    }

    /// Page of tree entries at `cs`, optionally filtered to a path prefix
    /// (`"src/"`-style prefixes match the directory subtree). Lazily
    /// materializes on a missing `tree_snap`; resume cursors are
    /// `{"k": last_index_key}`.
    pub async fn tree(
        &self,
        repo_id: &str,
        cs: &str,
        path_prefix: Option<&str>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<TreeEntry>, Option<String>), IndexError> {
        if self.db.is_skipped() {
            return Err(warned(IndexError::DbNotConfigured));
        }
        let row = self.changeset_row(repo_id, cs).await?;
        let cs = row.id;
        // "/" is ambiguous — the root listing is spelled `None`/"" — reject it
        // rather than silently coercing (S3).
        if path_prefix == Some("/") {
            return Err(invalid("prefix", "use empty for the root listing"));
        }
        // Prefix validation: a directory prefix may end in one `/`; strip it
        // before the path grammar check so "src/" is legal.
        // Normalize the filter to a directory prefix: non-empty filters end
        // in '/' so matching is always on a segment boundary — "src" selects
        // the file "src" and everything under "src/", never "srcb".
        let filter = match path_prefix {
            None | Some("") => String::new(),
            Some(p) => {
                let trimmed = p.strip_suffix('/').unwrap_or(p);
                if trimmed.is_empty() {
                    String::new()
                } else {
                    validate_read_path(trimmed)?;
                    format!("{trimmed}/")
                }
            }
        };
        if !self.tree_snap_exists(repo_id, &cs).await? {
            self.materialize(repo_id, &cs).await?;
        }
        let block = format!("{cs}\x01");
        let resume = match cursor {
            None => block.clone(),
            Some(raw) => decode_cursor::<KCursor>(raw)?.k,
        };
        let limit = limit.clamp(1, SCAN_PAGE as usize);

        let mut emitted: Vec<TreeEntry> = Vec::new();
        let mut last_emitted_key = String::new();
        let mut more = false;
        let mut cur = resume;
        'scan: loop {
            let rows = self.index_page(repo_id, KIND_TREE_ENTRY, &cur).await?;
            let fetched = rows.len();
            for r in rows {
                if !r.index_key.starts_with(&block) {
                    break 'scan;
                }
                cur = r.index_key.clone();
                let suffix = &r.index_key[block.len()..];
                // filter ends with '/' when non-empty: match the exact file
                // named by the bare prefix plus everything under it.
                let matched = filter.is_empty()
                    || suffix.starts_with(&filter)
                    || *suffix == filter[..filter.len() - 1];
                if matched {
                    if emitted.len() == limit {
                        more = true;
                        break 'scan;
                    }
                    let payload: TreeEntryPayload =
                        serde_json::from_str(&r.payload).map_err(|e| internal(e.to_string()))?;
                    emitted.push(TreeEntry {
                        path: suffix.to_string(),
                        mode: payload.mode,
                        blob_hash: payload.blob,
                    });
                    last_emitted_key = r.index_key.clone();
                } else if !filter.is_empty() && *suffix > *filter {
                    // Sorted past the contiguous match block.
                    break 'scan;
                }
            }
            if fetched < SCAN_PAGE as usize {
                break;
            }
        }
        let next = if more {
            Some(encode_cursor(&KCursor {
                k: last_emitted_key,
            }))
        } else {
            None
        };
        Ok((emitted, next))
    }

    /// File state at `cs` — `PathNotFound` on absent/deleted paths.
    pub async fn file_at(
        &self,
        repo_id: &str,
        cs: &str,
        path: &str,
    ) -> Result<FileEntry, IndexError> {
        if self.db.is_skipped() {
            return Err(warned(IndexError::DbNotConfigured));
        }
        let row = self.changeset_row(repo_id, cs).await?;
        validate_read_path(path)?;
        let (mode, blob) = self
            .file_state_at(repo_id, &row.id, path)
            .await?
            .ok_or_else(|| warned(IndexError::PathNotFound))?;
        Ok(FileEntry {
            mode,
            blob_hash: blob,
        })
    }

    // ------------------------------------------------------------------
    // path_history
    // ------------------------------------------------------------------

    /// Chronological touches of `path` — the `{hash32(path)}\x01` block in
    /// key order (committed_at embedded; same-second touches order by
    /// changeset id, SR4.7). Rename-away rows (`\x01r`) are visible with
    /// `old_path == path`.
    pub async fn path_history(
        &self,
        repo_id: &str,
        path: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<PathTouch>, Option<String>), IndexError> {
        if self.db.is_skipped() {
            return Err(warned(IndexError::DbNotConfigured));
        }
        validate_read_path(path)?;
        let block = format!("{}\x01", path_hash32(path));
        let resume = match cursor {
            None => block.clone(),
            Some(raw) => decode_cursor::<KCursor>(raw)?.k,
        };
        let limit = limit.clamp(1, SCAN_PAGE as usize);

        let mut emitted: Vec<PathTouch> = Vec::new();
        let mut last_emitted_key = String::new();
        let mut more = false;
        let mut cur = resume;
        'scan: loop {
            let rows = self.index_page(repo_id, KIND_PATH_TOUCH, &cur).await?;
            let fetched = rows.len();
            for r in rows {
                if !r.index_key.starts_with(&block) {
                    break 'scan;
                }
                cur = r.index_key.clone();
                let payload: PathTouchPayload =
                    serde_json::from_str(&r.payload).map_err(|e| internal(e.to_string()))?;
                if payload.path != path {
                    continue; // hashed-path guard (SR4.10)
                }
                if emitted.len() == limit {
                    more = true;
                    break 'scan;
                }
                emitted.push(PathTouch {
                    path: payload.path,
                    kind: ChangeKind::parse(&payload.kind).ok_or_else(|| {
                        internal(format!("stored path_touch kind: {}", payload.kind))
                    })?,
                    changeset_id: ChangesetId::parse(&payload.changeset_id).map_err(internal)?,
                    committed_at: payload.committed_at,
                    old_path: payload.old_path,
                    blob: payload.blob,
                    mode: payload.mode,
                });
                last_emitted_key = r.index_key.clone();
            }
            if fetched < SCAN_PAGE as usize {
                break;
            }
        }
        let next = if more {
            Some(encode_cursor(&KCursor {
                k: last_emitted_key,
            }))
        } else {
            None
        };
        Ok((emitted, next))
    }

    // ------------------------------------------------------------------
    // diff
    // ------------------------------------------------------------------

    /// Path-level delta between `from` and `to`. With a `paths` filter the
    /// candidate set is fixed; otherwise it is the union of both sides'
    /// touched paths between `merge_base(from,to)` and each tip — or the
    /// union of both trees when no base exists (disjoint histories are a
    /// full diff, not an error). Emitted in `BTreeSet` path order; the
    /// `{"k": last_path}` cursor resumes strictly after it.
    pub async fn diff(
        &self,
        repo_id: &str,
        from: &str,
        to: &str,
        paths: Option<&[String]>,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<DiffEntry>, Option<String>), IndexError> {
        if self.db.is_skipped() {
            return Err(warned(IndexError::DbNotConfigured));
        }
        let from_row = self.changeset_row(repo_id, from).await?;
        let to_row = self.changeset_row(repo_id, to).await?;
        let from = from_row.id;
        let to = to_row.id;

        let mut candidates: BTreeSet<String> = BTreeSet::new();
        match paths {
            Some(list) => {
                for p in list {
                    validate_read_path(p)?;
                    candidates.insert(p.clone());
                }
            }
            None => match self.merge_base(repo_id, &from, &to).await {
                Ok(base) => {
                    let base = base.as_str().to_string();
                    for tip in [&from, &to] {
                        self.collect_side_paths(repo_id, tip, &base, &mut candidates)
                            .await?;
                    }
                }
                Err(IndexError::BaseNotFound) => {
                    for tip in [&from, &to] {
                        let map = self.tree_map(repo_id, tip).await?;
                        candidates.extend(map.into_keys());
                    }
                }
                Err(e) => return Err(e),
            },
        }
        if candidates.len() > self.caps.diff_candidates_max {
            return Err(frontier());
        }

        let resume = match cursor {
            None => None,
            Some(raw) => Some(decode_cursor::<KCursor>(raw)?.k),
        };
        let limit = limit.clamp(1, SCAN_PAGE as usize);
        let mut emitted: Vec<DiffEntry> = Vec::new();
        let mut more = false;
        for path in &candidates {
            if let Some(k) = &resume {
                if path <= k {
                    continue;
                }
            }
            if emitted.len() == limit {
                more = true;
                break;
            }
            let a = self.file_state_at(repo_id, &from, path).await?;
            let b = self.file_state_at(repo_id, &to, path).await?;
            if a == b {
                continue;
            }
            let entry = match (&a, &b) {
                (None, Some((mode, blob))) => DiffEntry {
                    path: path.clone(),
                    kind: DiffKind::Add,
                    old_mode: None,
                    new_mode: Some(*mode),
                    old_blob: None,
                    new_blob: Some(blob.clone()),
                },
                (Some((mode, blob)), None) => DiffEntry {
                    path: path.clone(),
                    kind: DiffKind::Delete,
                    old_mode: Some(*mode),
                    new_mode: None,
                    old_blob: Some(blob.clone()),
                    new_blob: None,
                },
                (Some((om, ob)), Some((nm, nb))) => DiffEntry {
                    path: path.clone(),
                    kind: DiffKind::Modify,
                    old_mode: Some(*om),
                    new_mode: Some(*nm),
                    old_blob: Some(ob.clone()),
                    new_blob: Some(nb.clone()),
                },
                (None, None) => continue,
            };
            emitted.push(entry);
        }
        let next = if more {
            Some(encode_cursor(&KCursor {
                k: emitted.last().map(|e| e.path.clone()).unwrap_or_default(),
            }))
        } else {
            None
        };
        Ok((emitted, next))
    }

    /// Paths touched on `tip`'s side of `base` — manifests walked until the
    /// marked ancestor frontier; `old_path` included for renames.
    async fn collect_side_paths(
        &self,
        repo_id: &str,
        tip: &str,
        base: &str,
        out: &mut BTreeSet<String>,
    ) -> Result<(), IndexError> {
        // Mark base + ancestors so the side walk cannot bleed past it.
        let mut marked: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        marked.insert(base.to_string());
        queue.push_back(base.to_string());
        while let Some(x) = queue.pop_front() {
            for p in self.parents_of(repo_id, &x).await? {
                if marked.insert(p.clone()) {
                    if marked.len() > self.caps.diff_candidates_max {
                        return Err(frontier());
                    }
                    queue.push_back(p);
                }
            }
        }
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        visited.insert(tip.to_string());
        queue.push_back(tip.to_string());
        while let Some(x) = queue.pop_front() {
            if marked.contains(&x) {
                continue; // base-side ancestry is not a "side" change
            }
            let fcs = self.db.vcs_list_file_changes(&x).await.map_err(internal)?;
            for fc in fcs {
                out.insert(fc.path);
                if let Some(old) = fc.old_path {
                    out.insert(old);
                }
            }
            for p in self.parents_of(repo_id, &x).await? {
                if visited.insert(p.clone()) {
                    if visited.len() > self.caps.diff_candidates_max {
                        return Err(frontier());
                    }
                    queue.push_back(p);
                }
            }
        }
        Ok(())
    }

    /// Full tree map at `cs` — materializes on miss.
    async fn tree_map(
        &self,
        repo_id: &str,
        cs: &str,
    ) -> Result<BTreeMap<String, (i64, String)>, IndexError> {
        if !self.tree_snap_exists(repo_id, cs).await? {
            self.materialize(repo_id, cs).await?;
        }
        self.load_tree_entries(repo_id, cs).await
    }

    // ------------------------------------------------------------------
    // blame
    // ------------------------------------------------------------------

    /// Per-line attribution of `path` at `at`: touches of the path (plus the
    /// rename chain's old paths) ordered by ancestry distance from `at`
    /// oldest-first, diffed pairwise so surviving lines keep their
    /// introducer. Shard rows `{at}\x01{hash32}\x01{chunk:04}` (≤32KB
    /// payload) cache the result; a partial/corrupt shard set is recomputed.
    pub async fn blame(
        &self,
        repo_id: &str,
        path: &str,
        at: &str,
        limit: usize,
        cursor: Option<&str>,
    ) -> Result<(Vec<BlameLine>, Option<String>), IndexError> {
        if self.db.is_skipped() {
            return Err(warned(IndexError::DbNotConfigured));
        }
        let at_row = self.changeset_row(repo_id, at).await?;
        let at = at_row.id;
        validate_read_path(path)?;
        let block = format!("{at}\x01{}\x01", path_hash32(path));

        let mut shards = self.load_blame_shards(repo_id, path, &block).await?;
        if shards.is_none() {
            shards = Some(self.compute_blame(repo_id, path, &at).await?);
            self.write_blame_shards(repo_id, path, &at, shards.as_ref().unwrap())
                .await?;
        }
        let shards = shards.unwrap_or_default();

        // Page: flatten shards in key order; resume at {k: shard_key, off}.
        let resume: Option<BlameCursor> = match cursor {
            None => None,
            Some(raw) => Some(decode_cursor::<BlameCursor>(raw)?),
        };
        let limit = limit.clamp(1, SCAN_PAGE as usize);
        let mut emitted: Vec<BlameLine> = Vec::new();
        let mut next: Option<BlameCursor> = None;
        let mut started = resume.is_none();
        'outer: for (key, entries) in &shards {
            let start_off = match started {
                true => 0usize,
                false => {
                    match &resume {
                        Some(r) if key == &r.k => {
                            started = true;
                            r.off as usize
                        }
                        Some(r) if key.as_str() > r.k.as_str() => {
                            // Cursor's shard vanished — resume at the next
                            // surviving shard (never an oracle).
                            started = true;
                            0
                        }
                        _ => continue,
                    }
                }
            };
            for (i, e) in entries.iter().enumerate().skip(start_off) {
                if emitted.len() == limit {
                    next = Some(BlameCursor {
                        k: key.clone(),
                        off: i as u64,
                    });
                    break 'outer;
                }
                emitted.push(BlameLine {
                    line_no: e.n,
                    changeset_id: ChangesetId::parse(&e.c).map_err(internal)?,
                    author: e.a.clone(),
                    content: base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(&e.b)
                        .map_err(|e| internal(e.to_string()))?,
                });
            }
        }
        Ok((emitted, next.map(|n| encode_cursor(&n))))
    }

    /// `Some(shards)` when the shard set decodes whole and self-consistent;
    /// `None` → recompute (missing rows OR a partial write OR a payload
    /// whose `path`/`lines` disagree).
    async fn load_blame_shards(
        &self,
        repo_id: &str,
        path: &str,
        block: &str,
    ) -> Result<Option<Vec<(String, Vec<BlameShardEntry>)>>, IndexError> {
        let mut shards: Vec<(String, Vec<BlameShardEntry>)> = Vec::new();
        let mut total_lines: Option<u64> = None;
        let mut cur = block.to_string();
        loop {
            let rows = self.index_page(repo_id, KIND_BLAME, &cur).await?;
            let fetched = rows.len();
            let mut done = false;
            for r in rows {
                if !r.index_key.starts_with(block) {
                    done = true;
                    break;
                }
                cur = r.index_key.clone();
                let shard: BlameShard =
                    serde_json::from_str(&r.payload).map_err(|e| internal(e.to_string()))?;
                if shard.path != path {
                    continue; // hashed-path guard
                }
                match total_lines {
                    None => total_lines = Some(shard.lines),
                    Some(t) if t != shard.lines => return Ok(None),
                    _ => {}
                }
                shards.push((r.index_key, shard.entries));
            }
            if done || fetched < SCAN_PAGE as usize {
                break;
            }
        }
        if shards.is_empty() {
            return Ok(None);
        }
        let sum: u64 = shards.iter().map(|(_, e)| e.len() as u64).sum();
        if Some(sum) != total_lines {
            return Ok(None);
        }
        Ok(Some(shards))
    }

    async fn write_blame_shards(
        &self,
        repo_id: &str,
        path: &str,
        at: &str,
        shards: &[(String, Vec<BlameShardEntry>)],
    ) -> Result<(), IndexError> {
        let total: u64 = shards.iter().map(|(_, e)| e.len() as u64).sum();
        let payloads: Vec<String> = shards
            .iter()
            .map(|(_, entries)| {
                serde_json::to_string(&BlameShard {
                    path: path.to_string(),
                    lines: total,
                    entries: entries.clone(),
                })
                .map_err(|e| internal(e.to_string()))
            })
            .collect::<Result<_, _>>()?;
        // The computed key already carries `{at}\x01{hash32}\x01{chunk:04}`.
        let upserts: Vec<VcsDerivedIndexUpsert<'_>> = shards
            .iter()
            .zip(payloads.iter())
            .map(|((key, _), payload)| VcsDerivedIndexUpsert {
                kind: KIND_BLAME,
                index_key: key,
                payload,
                covering_changeset: at,
            })
            .collect();
        if !upserts.is_empty() {
            self.upsert_rows(repo_id, &upserts).await?;
            tracing::info!(
                event = "vcs.index.blame_shards",
                shards = upserts.len(),
                "blame shards written"
            );
        }
        Ok(())
    }

    /// Compute the blame for `path` at `at` and return shard-shaped chunks
    /// `(index_key, entries)` — key grammar `{at}\x01{hash32}\x01{chunk:04}`.
    async fn compute_blame(
        &self,
        repo_id: &str,
        path: &str,
        at: &str,
    ) -> Result<Vec<(String, Vec<BlameShardEntry>)>, IndexError> {
        let (mode, blob) = self
            .file_state_at(repo_id, at, path)
            .await?
            .ok_or_else(|| warned(IndexError::PathNotFound))?;
        let _ = mode;
        let bytes = self.object_bytes(&blob).await?;
        let lines_at = split_lines(&bytes);

        // Ancestry distance map (minimal hops from `at`) — also caches
        // author/committed_at per row.
        let dist = self.ancestor_rows(repo_id, at).await?;

        // Collect the touch chain (rename-chain aware, capped).
        let touches = self.collect_touches(repo_id, path).await?;

        let mut chain: Vec<&Touch> = touches
            .iter()
            .filter(|t| dist.contains_key(&t.changeset_id))
            .collect();
        chain.sort_by(|a, b| {
            let da = dist[&a.changeset_id].0;
            let db = dist[&b.changeset_id].0;
            // Oldest first: greater distance, then committed_at, then id.
            db.cmp(&da)
                .then(a.committed_at.cmp(&b.committed_at))
                .then(a.changeset_id.cmp(&b.changeset_id))
        });

        // Diff-chain attribution: lines surviving a diff keep their
        // introducer; newly appearing lines attribute to the current touch.
        let mut prev_lines: Vec<Vec<u8>> = Vec::new();
        let mut prev_attr: Vec<(String, String)> = Vec::new();
        for t in &chain {
            let cur_lines = match &t.blob {
                Some(h) => split_lines(&self.object_bytes(h).await?),
                None => Vec::new(),
            };
            let author = dist
                .get(&t.changeset_id)
                .map(|(_, _, a)| a.clone())
                .unwrap_or_default();
            let cur_attr = self.carry_attr(
                &prev_lines,
                &cur_lines,
                &prev_attr,
                (t.changeset_id.clone(), author),
            );
            prev_lines = cur_lines;
            prev_attr = cur_attr;
        }
        // Final step to the blob at `at`. Unmatched tail lines take the
        // oldest-toucher fallback (pinned ambiguity rule); with no touches
        // at all, `at` itself introduces them.
        let fallback = chain
            .first()
            .map(|t| {
                (
                    t.changeset_id.clone(),
                    dist.get(&t.changeset_id)
                        .map(|(_, _, a)| a.clone())
                        .unwrap_or_default(),
                )
            })
            .unwrap_or_else(|| {
                (
                    at.to_string(),
                    dist.get(at).map(|(_, _, a)| a.clone()).unwrap_or_default(),
                )
            });
        let final_attr = self.carry_attr(&prev_lines, &lines_at, &prev_attr, fallback);

        // Shard: greedily chunk serialized entries under 32KB. Size is
        // accounted incrementally — base envelope + per-entry JSON length —
        // instead of re-serializing the whole chunk per line.
        let block = format!("{at}\x01{}\x01", path_hash32(path));
        let base_len = serde_json::to_vec(&BlameShard {
            path: path.to_string(),
            lines: lines_at.len() as u64,
            entries: Vec::new(),
        })
        .map_err(|e| internal(e.to_string()))?
        .len();
        let mut shards: Vec<(String, Vec<BlameShardEntry>)> = Vec::new();
        let mut chunk: Vec<BlameShardEntry> = Vec::new();
        // +1 per entry covers the array comma; the first entry over-counts
        // by one byte — a conservative split, never an oversize shard.
        let mut chunk_len = base_len;
        let mut chunk_idx = 0usize;
        for (i, line) in lines_at.iter().enumerate() {
            let (cs, author) = &final_attr[i];
            let entry = BlameShardEntry {
                n: i as u64 + 1,
                c: cs.clone(),
                a: author.clone(),
                b: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(line),
            };
            let entry_len = serde_json::to_vec(&entry)
                .map_err(|e| internal(e.to_string()))?
                .len()
                + 1;
            if chunk_len + entry_len > BLAME_SHARD_MAX && !chunk.is_empty() {
                shards.push((format!("{block}{chunk_idx:04}"), std::mem::take(&mut chunk)));
                chunk_idx += 1;
                chunk_len = base_len;
            }
            chunk_len += entry_len;
            chunk.push(entry);
        }
        if !chunk.is_empty() {
            shards.push((format!("{block}{chunk_idx:04}"), chunk));
        }
        Ok(shards)
    }

    /// Map prev→cur line attribution through the Myers hunks: equal runs
    /// carry attribution pairwise; inserted regions take `introducer`.
    fn carry_attr(
        &self,
        prev_lines: &[Vec<u8>],
        cur_lines: &[Vec<u8>],
        prev_attr: &[(String, String)],
        introducer: (String, String),
    ) -> Vec<(String, String)> {
        let prev_refs: Vec<&[u8]> = prev_lines.iter().map(|v| v.as_slice()).collect();
        let cur_refs: Vec<&[u8]> = cur_lines.iter().map(|v| v.as_slice()).collect();
        let hunks = diff_lines(&prev_refs, &cur_refs);
        let mut cur_attr = vec![introducer; cur_lines.len()];
        let (mut o, mut n) = (0usize, 0usize);
        for h in &hunks {
            let run = (h.old_start - o).min(h.new_start - n);
            for i in 0..run {
                if o + i < prev_attr.len() && n + i < cur_attr.len() {
                    cur_attr[n + i] = prev_attr[o + i].clone();
                }
            }
            o = h.old_start + h.old_len;
            n = h.new_start + h.new_len;
        }
        let run = (prev_lines.len() - o).min(cur_lines.len() - n);
        for i in 0..run {
            if o + i < prev_attr.len() && n + i < cur_attr.len() {
                cur_attr[n + i] = prev_attr[o + i].clone();
            }
        }
        cur_attr
    }

    /// Path-touch chain for blame — the `{hash32(path)}\x01` block plus
    /// transitive old-path blocks of renames (each hop counts toward
    /// `blame_touches_max`). Rename-away `\x01r` rows dedupe against their
    /// new-path row by changeset id.
    async fn collect_touches(&self, repo_id: &str, path: &str) -> Result<Vec<Touch>, IndexError> {
        let mut queue: VecDeque<String> = VecDeque::new();
        let mut visited_paths: HashSet<String> = HashSet::new();
        let mut seen_cs: HashSet<String> = HashSet::new();
        let mut out: Vec<Touch> = Vec::new();
        queue.push_back(path.to_string());
        while let Some(p) = queue.pop_front() {
            if !visited_paths.insert(p.clone()) {
                continue;
            }
            let block = format!("{}\x01", path_hash32(&p));
            let mut cur = block.clone();
            loop {
                let rows = self.index_page(repo_id, KIND_PATH_TOUCH, &cur).await?;
                let fetched = rows.len();
                let mut done = false;
                for r in rows {
                    if !r.index_key.starts_with(&block) {
                        done = true;
                        break;
                    }
                    cur = r.index_key.clone();
                    let payload: PathTouchPayload =
                        serde_json::from_str(&r.payload).map_err(|e| internal(e.to_string()))?;
                    if payload.path != p {
                        continue;
                    }
                    if !seen_cs.insert(payload.changeset_id.clone()) {
                        continue; // r-row vs normal-row dedupe
                    }
                    if out.len() >= self.caps.blame_touches_max {
                        return Err(frontier());
                    }
                    let is_rename = payload.kind == "rename";
                    // Normal rename rows chain backward to old_path; the
                    // `\x01r` rename-away rows have old_path == their own
                    // path (self-link) and never hop.
                    let hop = if is_rename && payload.old_path.as_deref() != Some(p.as_str()) {
                        payload.old_path.clone()
                    } else {
                        None
                    };
                    out.push(Touch {
                        changeset_id: payload.changeset_id,
                        committed_at: payload.committed_at,
                        blob: payload.blob,
                    });
                    if let Some(hop) = hop {
                        queue.push_back(hop);
                    }
                }
                if done || fetched < SCAN_PAGE as usize {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// BFS over parents from `at` — `{id: (distance, committed_at, author)}`.
    /// Minimal-hop distance orders the blame chain oldest-first.
    async fn ancestor_rows(
        &self,
        repo_id: &str,
        at: &str,
    ) -> Result<HashMap<String, (usize, String, String)>, IndexError> {
        let mut map: HashMap<String, (usize, String, String)> = HashMap::new();
        let mut queue: VecDeque<(String, usize)> = VecDeque::new();
        queue.push_back((at.to_string(), 0));
        while let Some((id, d)) = queue.pop_front() {
            if map.contains_key(&id) {
                continue;
            }
            let row = self
                .db
                .vcs_get_changeset(repo_id, &id)
                .await
                .map_err(internal)?
                .ok_or_else(|| internal("ancestor vanished mid-walk".to_string()))?;
            map.insert(
                id.clone(),
                (d, row.committed_at.clone(), row.author.clone()),
            );
            if map.len() > self.caps.mark_max {
                return Err(frontier());
            }
            for p in self.parents_of(repo_id, &id).await? {
                if !map.contains_key(&p) {
                    queue.push_back((p, d + 1));
                }
            }
        }
        Ok(map)
    }

    async fn object_bytes(&self, hash: &str) -> Result<Vec<u8>, IndexError> {
        let mut file = self.objects.get(hash).await.map_err(|e| match e {
            ObjectStoreError::DbNotConfigured => warned(IndexError::DbNotConfigured),
            // Manifest-covered blobs are GC-pinned — a store miss or I/O
            // failure here is a store inconsistency, opaque Internal.
            e => internal(e.to_string()),
        })?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)
            .await
            .map_err(|e| internal(e.to_string()))?;
        Ok(buf)
    }
}

/// One collected `path_touch` event for the blame chain — author is looked
/// up from the ancestry map at chain-build time (a touch reachable only
/// through non-ancestor branches is filtered before use anyway).
struct Touch {
    changeset_id: String,
    committed_at: String,
    blob: Option<String>,
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::changeset_store::CreateChangesetInput;
    use crate::object_store::testutil::{cfg, seed_repo, shard_file, test_db};
    use crate::object_store::ObjectStore;

    fn blob(byte: u8) -> String {
        format!("{:02x}", byte).repeat(32)
    }

    struct Rig {
        _dbdir: tempfile::TempDir,
        odir: tempfile::TempDir,
        db: Database,
        store: ChangesetStore,
        repo: String,
    }

    async fn rig(caps: ReadIndexCaps) -> (Rig, ReadIndex) {
        let (dbdir, db) = test_db().await;
        let repo = seed_repo(&db, "u-ri", "r-ri").await;
        let odir = tempfile::tempdir().unwrap();
        let objects = ObjectStore::new(db.clone(), cfg(odir.path())).unwrap();
        let index = ReadIndex::new(db.clone(), objects, caps);
        (
            Rig {
                _dbdir: dbdir,
                odir,
                db: db.clone(),
                store: ChangesetStore::new(db),
                repo,
            },
            index,
        )
    }

    fn fc(path: &str, kind: ChangeKind, blob: Option<String>) -> FileChangeInput {
        FileChangeInput {
            path: path.to_string(),
            kind,
            mode: 33188,
            old_path: None,
            blob_hash: blob,
        }
    }

    async fn cs(
        r: &Rig,
        parents: &[&str],
        msg: &str,
        ts: &str,
        changes: Vec<FileChangeInput>,
    ) -> ChangesetId {
        // Create validates blob linkage; seed object rows for any hash not
        // already seeded with real bytes (insert-if-absent keeps those).
        for c in &changes {
            if let Some(h) = &c.blob_hash {
                r.db.vcs_insert_object_if_absent(h, 1).await.unwrap();
            }
        }
        r.store
            .create(CreateChangesetInput {
                repo_id: r.repo.clone(),
                parents: parents.iter().map(|p| p.to_string()).collect(),
                author_name: "T".into(),
                author_email: "t@example.com".into(),
                message: msg.into(),
                committed_at: Some(ts.to_string()),
                changes,
            })
            .await
            .unwrap_or_else(|e| panic!("create {msg}: {e}"))
    }

    /// Seed an object row + on-disk bytes (blame/diff reads go through the
    /// store — content must exist on both sides).
    async fn seed_object(r: &Rig, hash: &str, bytes: &[u8]) {
        r.db.vcs_insert_object_if_absent(hash, bytes.len() as i64)
            .await
            .unwrap();
        shard_file(r.odir.path(), hash, bytes);
    }

    // ---- update_for row shapes ----

    #[test]
    fn update_for_key_and_payload_grammar() {
        let changes = vec![
            fc("src/a.rs", ChangeKind::Add, Some(blob(0xa1))),
            FileChangeInput {
                path: "new/x".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: Some("old/x".into()),
                blob_hash: Some(blob(0xa2)),
            },
        ];
        let rows = ReadIndex::update_for(
            "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR",
            "2026-01-02T03:04:05Z",
            &changes,
        );
        assert_eq!(rows.len(), 3, "rename produces a second r-row");
        let expected = format!(
            "{}\x012026-01-02T03:04:05Z\x01cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR",
            path_hash32("src/a.rs")
        );
        assert_eq!(rows[0].index_key, expected);
        let p: serde_json::Value = serde_json::from_str(&rows[0].payload).unwrap();
        assert_eq!(p["path"], "src/a.rs");
        assert_eq!(p["kind"], "add");
        assert_eq!(p["changeset_id"], "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR");
        assert_eq!(p["committed_at"], "2026-01-02T03:04:05Z");
        assert_eq!(p["mode"], 33188);
        // rename: normal row keyed on new path, r-row on old with \x01r.
        let normal = &rows[1];
        let r_row = &rows[2];
        assert!(normal
            .index_key
            .starts_with(&format!("{}\x01", path_hash32("new/x"))));
        assert!(r_row.index_key.ends_with("\x01r"));
        assert!(r_row
            .index_key
            .starts_with(&format!("{}\x01", path_hash32("old/x"))));
        let rp: serde_json::Value = serde_json::from_str(&r_row.payload).unwrap();
        assert_eq!(rp["path"], "old/x");
        assert_eq!(rp["old_path"], "old/x");
        assert_eq!(rp["kind"], "rename");
    }

    // ---- log ----

    #[tokio::test]
    async fn log_pages_no_dup_and_from_sources() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let c1 = cs(
            &r,
            &[],
            "one",
            "2026-01-01T00:00:00Z",
            vec![fc("a", ChangeKind::Add, Some(blob(0xa1)))],
        )
        .await;
        // Same-second commits — the skip-replay cursor must not duplicate.
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "two",
            "2026-01-01T00:00:00Z",
            vec![fc("b", ChangeKind::Add, Some(blob(0xa2)))],
        )
        .await;
        let c3 = cs(
            &r,
            &[c2.as_str()],
            "three",
            "2026-01-01T00:00:00Z",
            vec![fc("c", ChangeKind::Add, Some(blob(0xa3)))],
        )
        .await;

        let (page, next) = index.log(&r.repo, c3.as_str(), 2, None).await.unwrap();
        assert_eq!(page.len(), 2);
        let next = next.expect("cursor");
        let (page2, next2) = index
            .log(&r.repo, c3.as_str(), 2, Some(&next))
            .await
            .unwrap();
        let all: Vec<String> = page
            .iter()
            .chain(page2.iter())
            .map(|e| e.id.as_str().to_string())
            .collect();
        assert_eq!(all.len(), 3, "no duplicates across pages");
        assert!(next2.is_none());
        assert!(all.contains(&c1.as_str().to_string()));
        assert!(all.contains(&c2.as_str().to_string()));
        assert!(all.contains(&c3.as_str().to_string()));

        // from a ref name → resolves via ref scan.
        RefRegistry::new(r.db.clone())
            .cas(&r.repo, "main", None, Some(c3.as_str()))
            .await
            .unwrap();
        let (entries, _) = index.log(&r.repo, "main", 10, None).await.unwrap();
        assert_eq!(entries.len(), 3);
        // from a git oid via import link.
        let oid = "ef".repeat(20);
        r.db.vcs_insert_import_link_if_absent(&r.repo, &oid, c2.as_str())
            .await
            .unwrap();
        let (entries, _) = index.log(&r.repo, &oid, 10, None).await.unwrap();
        assert_eq!(entries.len(), 2, "git-oid from resolves to c2 ancestry");
        // unknown ref → NotFound; malformed cursor → CursorMalformed.
        assert_eq!(
            index.log(&r.repo, "no-such-ref", 10, None).await,
            Err(IndexError::NotFound)
        );
        assert_eq!(
            index.log(&r.repo, c3.as_str(), 10, Some("!!!")).await,
            Err(IndexError::CursorMalformed)
        );
        // wrong-shape decodable cursor → CursorMalformed too.
        let wrong = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"k":"x"}"#);
        assert_eq!(
            index.log(&r.repo, c3.as_str(), 10, Some(&wrong)).await,
            Err(IndexError::CursorMalformed)
        );
        // nonexistent changeset id → NotFound; malformed id → InvalidInput.
        assert_eq!(
            index
                .log(&r.repo, "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR", 10, None)
                .await,
            Err(IndexError::NotFound)
        );
        assert!(matches!(
            index.log(&r.repo, "not-anything", 10, None).await,
            Err(IndexError::NotFound) // falls to ref resolution → absent
        ));
    }

    #[tokio::test]
    async fn log_seen_cap_trips() {
        // seen_max=2 → seeding 3 nodes must trip.
        let (r, index) = rig(ReadIndexCaps {
            seen_max: 2,
            ..ReadIndexCaps::default()
        })
        .await;
        let c1 = cs(
            &r,
            &[],
            "one",
            "2026-01-01T00:00:00Z",
            vec![fc("a", ChangeKind::Add, Some(blob(0xa1)))],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "two",
            "2026-01-01T00:01:00Z",
            vec![fc("b", ChangeKind::Add, Some(blob(0xa2)))],
        )
        .await;
        let c3 = cs(
            &r,
            &[c2.as_str()],
            "three",
            "2026-01-01T00:02:00Z",
            vec![fc("c", ChangeKind::Add, Some(blob(0xa3)))],
        )
        .await;
        assert_eq!(
            index.log(&r.repo, c3.as_str(), 10, None).await,
            Err(IndexError::FrontierTooLarge)
        );
    }

    // ---- merge_base ----

    #[tokio::test]
    async fn merge_base_linear_divergent_disjoint() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let c1 = cs(
            &r,
            &[],
            "root",
            "2026-01-01T00:00:00Z",
            vec![fc("a", ChangeKind::Add, Some(blob(0xa1)))],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "mid",
            "2026-01-01T00:01:00Z",
            vec![fc("b", ChangeKind::Add, Some(blob(0xa2)))],
        )
        .await;
        // linear: base(descendant, ancestor) = ancestor.
        let base = index
            .merge_base(&r.repo, c2.as_str(), c1.as_str())
            .await
            .unwrap();
        assert_eq!(base, c1);
        // symmetric: arg order doesn't matter.
        assert_eq!(
            index
                .merge_base(&r.repo, c1.as_str(), c2.as_str())
                .await
                .unwrap(),
            c1
        );
        // divergent diamond: two children of c2 merge at c5.
        let l = cs(
            &r,
            &[c2.as_str()],
            "left",
            "2026-01-01T00:02:00Z",
            vec![fc("l", ChangeKind::Add, Some(blob(0xa4)))],
        )
        .await;
        let rr = cs(
            &r,
            &[c2.as_str()],
            "right",
            "2026-01-01T00:03:00Z",
            vec![fc("r", ChangeKind::Add, Some(blob(0xa5)))],
        )
        .await;
        let merge = cs(
            &r,
            &[l.as_str(), rr.as_str()],
            "merge",
            "2026-01-01T00:04:00Z",
            vec![fc("m", ChangeKind::Add, Some(blob(0xa6)))],
        )
        .await;
        let base = index
            .merge_base(&r.repo, l.as_str(), rr.as_str())
            .await
            .unwrap();
        assert_eq!(base, c2, "diamond base must be c2");
        // merge vs a deeper side: base(l, merge) = l.
        assert_eq!(
            index
                .merge_base(&r.repo, l.as_str(), merge.as_str())
                .await
                .unwrap(),
            l
        );
        // disjoint roots → BaseNotFound.
        let x = cs(
            &r,
            &[],
            "x",
            "2026-01-01T00:05:00Z",
            vec![fc("x", ChangeKind::Add, Some(blob(0xa7)))],
        )
        .await;
        assert_eq!(
            index.merge_base(&r.repo, x.as_str(), c1.as_str()).await,
            Err(IndexError::BaseNotFound)
        );
        // Cached row serves the second identical call — still correct.
        let base2 = index
            .merge_base(&r.repo, l.as_str(), rr.as_str())
            .await
            .unwrap();
        assert_eq!(base2, c2);
        // malformed id → InvalidInput; missing → NotFound.
        assert!(matches!(
            index.merge_base(&r.repo, "junk", c1.as_str()).await,
            Err(IndexError::InvalidInput(_))
        ));
        assert_eq!(
            index
                .merge_base(&r.repo, "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR", c1.as_str())
                .await,
            Err(IndexError::NotFound)
        );
    }

    #[tokio::test]
    async fn merge_base_mark_cap_trips() {
        let (r, index) = rig(ReadIndexCaps {
            mark_max: 2,
            ..ReadIndexCaps::default()
        })
        .await;
        let c1 = cs(
            &r,
            &[],
            "one",
            "2026-01-01T00:00:00Z",
            vec![fc("a", ChangeKind::Add, Some(blob(0xa1)))],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "two",
            "2026-01-01T00:01:00Z",
            vec![fc("b", ChangeKind::Add, Some(blob(0xa2)))],
        )
        .await;
        let c3 = cs(
            &r,
            &[c2.as_str()],
            "three",
            "2026-01-01T00:02:00Z",
            vec![fc("c", ChangeKind::Add, Some(blob(0xa3)))],
        )
        .await;
        let x = cs(
            &r,
            &[],
            "x",
            "2026-01-01T00:03:00Z",
            vec![fc("x", ChangeKind::Add, Some(blob(0xa4)))],
        )
        .await;
        // marking lo's ancestors exceeds mark_max=2 (lo + c3 + c2…).
        assert_eq!(
            index.merge_base(&r.repo, c3.as_str(), x.as_str()).await,
            Err(IndexError::FrontierTooLarge)
        );
    }

    // ---- tree / file_at ----

    #[tokio::test]
    async fn tree_materialize_prefix_paginate_resume() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let b1 = blob(0xa1);
        let b2 = blob(0xa2);
        for h in [&b1, &b2] {
            r.db.vcs_insert_object_if_absent(h, 4).await.unwrap();
        }
        let c1 = cs(
            &r,
            &[],
            "root",
            "2026-01-01T00:00:00Z",
            vec![
                fc("src/a.rs", ChangeKind::Add, Some(b1.clone())),
                fc("src/b.rs", ChangeKind::Add, Some(b1.clone())),
                fc("top.txt", ChangeKind::Add, Some(b2.clone())),
            ],
        )
        .await;
        let (entries, next) = index
            .tree(&r.repo, c1.as_str(), None, 10, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 3);
        assert!(next.is_none());
        // entries ordered by index_key — path order.
        assert_eq!(entries[0].path, "src/a.rs");
        // snap now exists → second call serves from index.
        assert!(index.tree_snap_exists(&r.repo, c1.as_str()).await.unwrap());
        // prefix filter "src/" → only the two src entries.
        let (entries, _) = index
            .tree(&r.repo, c1.as_str(), Some("src/"), 10, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
        // prefix "src" (no slash) also selects the subtree + literal file.
        let (entries, _) = index
            .tree(&r.repo, c1.as_str(), Some("src"), 10, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
        // a file-path prefix selects itself.
        let (entries, _) = index
            .tree(&r.repo, c1.as_str(), Some("top.txt"), 10, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        // pagination: limit=1 pages through.
        let (p1, n1) = index
            .tree(&r.repo, c1.as_str(), None, 1, None)
            .await
            .unwrap();
        assert_eq!(p1.len(), 1);
        let (p2, n2) = index
            .tree(&r.repo, c1.as_str(), None, 1, Some(&n1.unwrap()))
            .await
            .unwrap();
        assert_eq!(p2.len(), 1);
        let (p3, n3) = index
            .tree(&r.repo, c1.as_str(), None, 1, Some(&n2.unwrap()))
            .await
            .unwrap();
        assert_eq!(p3.len(), 1);
        assert!(n3.is_none());
        let paths: Vec<&str> = p1
            .iter()
            .chain(p2.iter())
            .chain(p3.iter())
            .map(|e| e.path.as_str())
            .collect();
        assert_eq!(paths, ["src/a.rs", "src/b.rs", "top.txt"]);
    }

    #[tokio::test]
    async fn tree_crash_resume_entries_without_snap() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let h = blob(0xa1);
        r.db.vcs_insert_object_if_absent(&h, 4).await.unwrap();
        let c1 = cs(
            &r,
            &[],
            "root",
            "2026-01-01T00:00:00Z",
            vec![fc("a", ChangeKind::Add, Some(h.clone()))],
        )
        .await;
        // Simulate a mid-write crash: a stale/partial entry row exists
        // without the snap marker — recompute must overwrite cleanly.
        r.db.vcs_upsert_derived_index_rows(
            &r.repo,
            &[VcsDerivedIndexUpsert {
                kind: KIND_TREE_ENTRY,
                index_key: &format!("{}\x01a", c1.as_str()),
                payload: "{\"mode\":33188,\"blob\":\"WRONG\"}",
                covering_changeset: c1.as_str(),
            }],
        )
        .await
        .unwrap();
        let (entries, _) = index
            .tree(&r.repo, c1.as_str(), None, 10, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].blob_hash, h,
            "recompute must overwrite stale entry"
        );
        assert!(index.tree_snap_exists(&r.repo, c1.as_str()).await.unwrap());
    }

    #[tokio::test]
    async fn file_at_states() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let h1 = blob(0xa1);
        let h2 = blob(0xa2);
        for h in [&h1, &h2] {
            r.db.vcs_insert_object_if_absent(h, 4).await.unwrap();
        }
        let c1 = cs(
            &r,
            &[],
            "root",
            "2026-01-01T00:00:00Z",
            vec![
                fc("keep", ChangeKind::Add, Some(h1.clone())),
                fc("gone", ChangeKind::Add, Some(h1.clone())),
            ],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "del",
            "2026-01-01T00:01:00Z",
            vec![
                fc("keep", ChangeKind::Modify, Some(h2.clone())),
                fc("gone", ChangeKind::Delete, None),
            ],
        )
        .await;
        // at c1
        let e = index.file_at(&r.repo, c1.as_str(), "gone").await.unwrap();
        assert_eq!(e.blob_hash, h1);
        // at c2: keep modified, gone deleted → PathNotFound.
        let e = index.file_at(&r.repo, c2.as_str(), "keep").await.unwrap();
        assert_eq!(e.blob_hash, h2);
        assert_eq!(
            index.file_at(&r.repo, c2.as_str(), "gone").await,
            Err(IndexError::PathNotFound)
        );
        assert_eq!(
            index.file_at(&r.repo, c2.as_str(), "never").await,
            Err(IndexError::PathNotFound)
        );
        // Snap-backed read at c1 after materializing c2's tree: the fold
        // must still see c1's version.
        index
            .tree(&r.repo, c2.as_str(), None, 10, None)
            .await
            .unwrap();
        let e = index.file_at(&r.repo, c1.as_str(), "keep").await.unwrap();
        assert_eq!(e.blob_hash, h1);
    }

    #[tokio::test]
    async fn chained_rename_in_one_changeset_no_phantoms() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let b1 = blob(0xa1);
        let b2 = blob(0xa2);
        r.db.vcs_insert_object_if_absent(&b1, 4).await.unwrap();
        r.db.vcs_insert_object_if_absent(&b2, 4).await.unwrap();
        let c1 = cs(
            &r,
            &[],
            "seed",
            "2026-01-01T00:00:00Z",
            vec![fc("a.txt", ChangeKind::Add, Some(b1.clone()))],
        )
        .await;
        // One changeset renames a.txt→b.txt AND b.txt→c.txt: b.txt is both a
        // rename target and a removal source — the sequential fold must not
        // leave it behind (move-and-leave-copy was the review bug).
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "chain",
            "2026-01-01T00:01:00Z",
            vec![
                FileChangeInput {
                    path: "b.txt".into(),
                    kind: ChangeKind::Rename,
                    mode: 33188,
                    old_path: Some("a.txt".into()),
                    blob_hash: Some(b1.clone()),
                },
                FileChangeInput {
                    path: "c.txt".into(),
                    kind: ChangeKind::Rename,
                    mode: 33188,
                    old_path: Some("b.txt".into()),
                    blob_hash: Some(b2.clone()),
                },
            ],
        )
        .await;
        // file_at: the middle hop is absent; the tail carries its own blob.
        assert_eq!(
            index.file_at(&r.repo, c2.as_str(), "b.txt").await,
            Err(IndexError::PathNotFound)
        );
        assert_eq!(
            index.file_at(&r.repo, c2.as_str(), "a.txt").await,
            Err(IndexError::PathNotFound)
        );
        let e = index.file_at(&r.repo, c2.as_str(), "c.txt").await.unwrap();
        assert_eq!(e.blob_hash, b2);
        // tree: only c.txt survives the chain.
        let (entries, next) = index
            .tree(&r.repo, c2.as_str(), None, 10, None)
            .await
            .unwrap();
        assert!(next.is_none());
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["c.txt"]);
        assert_eq!(entries[0].blob_hash, b2);
    }

    // ---- path_history ----

    #[tokio::test]
    async fn path_history_chronological_and_rename() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let h = blob(0xa1);
        r.db.vcs_insert_object_if_absent(&h, 4).await.unwrap();
        let c1 = cs(
            &r,
            &[],
            "add",
            "2026-01-01T00:00:00Z",
            vec![fc("old/p.rs", ChangeKind::Add, Some(h.clone()))],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "mod",
            "2026-01-01T00:01:00Z",
            vec![fc("old/p.rs", ChangeKind::Modify, Some(h.clone()))],
        )
        .await;
        let c3 = cs(
            &r,
            &[c2.as_str()],
            "ren",
            "2026-01-01T00:02:00Z",
            vec![FileChangeInput {
                path: "new/p.rs".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: Some("old/p.rs".into()),
                blob_hash: Some(h.clone()),
            }],
        )
        .await;
        let (touches, next) = index
            .path_history(&r.repo, "old/p.rs", 10, None)
            .await
            .unwrap();
        assert!(next.is_none());
        // add + modify + rename-away (r-row) = 3 touches, chronological.
        assert_eq!(touches.len(), 3);
        assert_eq!(touches[0].changeset_id, c1);
        assert_eq!(touches[1].changeset_id, c2);
        assert_eq!(touches[2].changeset_id, c3);
        assert_eq!(touches[2].kind, ChangeKind::Rename);
        assert_eq!(touches[2].old_path.as_deref(), Some("old/p.rs"));
        // new path sees the rename event (normal row).
        let (touches, _) = index
            .path_history(&r.repo, "new/p.rs", 10, None)
            .await
            .unwrap();
        assert_eq!(touches.len(), 1);
        assert_eq!(touches[0].old_path.as_deref(), Some("old/p.rs"));
        // payload.path guard: inject a mismatched row inside the block —
        // must be filtered, never emitted.
        let forged_key = format!(
            "{}\x012026-01-01T00:03:00Z\x01{}",
            path_hash32("old/p.rs"),
            c3.as_str()
        );
        r.db.vcs_upsert_derived_index_rows(
            &r.repo,
            &[VcsDerivedIndexUpsert {
                kind: KIND_PATH_TOUCH,
                index_key: &forged_key,
                payload: "{\"path\":\"OTHER/p.rs\",\"kind\":\"add\",\"changeset_id\":\"x\",\"committed_at\":\"t\",\"mode\":33188}",
                covering_changeset: c3.as_str(),
            }],
        )
        .await
        .unwrap();
        let (touches, _) = index
            .path_history(&r.repo, "old/p.rs", 10, None)
            .await
            .unwrap();
        assert_eq!(touches.len(), 3, "mismatched payload must be filtered");
        // malformed cursor.
        assert_eq!(
            index
                .path_history(&r.repo, "old/p.rs", 10, Some("~bad"))
                .await,
            Err(IndexError::CursorMalformed)
        );
        // invalid path → InvalidInput.
        assert!(matches!(
            index.path_history(&r.repo, "a/../b", 10, None).await,
            Err(IndexError::InvalidInput(_))
        ));
    }

    // ---- diff ----

    #[tokio::test]
    async fn diff_candidates_and_ordering() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        for h in [&blob(0xa1), &blob(0xa2), &blob(0xa3)] {
            r.db.vcs_insert_object_if_absent(h, 4).await.unwrap();
        }
        let c1 = cs(
            &r,
            &[],
            "base",
            "2026-01-01T00:00:00Z",
            vec![
                fc("mod", ChangeKind::Add, Some(blob(0xa1))),
                fc("del", ChangeKind::Add, Some(blob(0xa1))),
                fc("same", ChangeKind::Add, Some(blob(0xa1))),
            ],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "tip",
            "2026-01-01T00:01:00Z",
            vec![
                fc("mod", ChangeKind::Modify, Some(blob(0xa2))),
                fc("del", ChangeKind::Delete, None),
                fc("add", ChangeKind::Add, Some(blob(0xa3))),
            ],
        )
        .await;
        // from → to.
        let (entries, next) = index
            .diff(&r.repo, c1.as_str(), c2.as_str(), None, 10, None)
            .await
            .unwrap();
        assert!(next.is_none());
        let kinds: Vec<(&str, &str)> = entries
            .iter()
            .map(|e| (e.path.as_str(), e.kind.as_str()))
            .collect();
        // BTreeSet path order: add < del < mod.
        assert_eq!(
            kinds,
            vec![("add", "add"), ("del", "delete"), ("mod", "modify")],
            "diff must emit in BTreeSet path order"
        );
        let m = entries.iter().find(|e| e.path == "mod").unwrap();
        assert_eq!(m.old_blob.as_deref(), Some(blob(0xa1).as_str()));
        assert_eq!(m.new_blob.as_deref(), Some(blob(0xa2).as_str()));
        // paths filter narrows candidates.
        let (entries, _) = index
            .diff(
                &r.repo,
                c1.as_str(),
                c2.as_str(),
                Some(&["del".to_string()]),
                10,
                None,
            )
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
        // reverse direction flips kinds.
        let (entries, _) = index
            .diff(&r.repo, c2.as_str(), c1.as_str(), None, 10, None)
            .await
            .unwrap();
        let kinds: Vec<(&str, &str)> = entries
            .iter()
            .map(|e| (e.path.as_str(), e.kind.as_str()))
            .collect();
        assert_eq!(
            kinds,
            vec![("add", "delete"), ("del", "add"), ("mod", "modify")]
        );
        // pagination across entries.
        let (p1, n1) = index
            .diff(&r.repo, c1.as_str(), c2.as_str(), None, 2, None)
            .await
            .unwrap();
        let (p2, _) = index
            .diff(
                &r.repo,
                c1.as_str(),
                c2.as_str(),
                None,
                2,
                Some(&n1.unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(p1.len() + p2.len(), 3);
    }

    #[tokio::test]
    async fn diff_disjoint_histories_full_delta() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        r.db.vcs_insert_object_if_absent(&blob(0xa1), 4)
            .await
            .unwrap();
        let c1 = cs(
            &r,
            &[],
            "a",
            "2026-01-01T00:00:00Z",
            vec![fc("x", ChangeKind::Add, Some(blob(0xa1)))],
        )
        .await;
        let c2 = cs(
            &r,
            &[],
            "b",
            "2026-01-01T00:01:00Z",
            vec![fc("y", ChangeKind::Add, Some(blob(0xa1)))],
        )
        .await;
        // No base → full tree diff (BaseNotFound falls back, never errors).
        let (entries, _) = index
            .diff(&r.repo, c1.as_str(), c2.as_str(), None, 10, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn diff_candidates_cap_trips() {
        let (r, index) = rig(ReadIndexCaps {
            diff_candidates_max: 1,
            ..ReadIndexCaps::default()
        })
        .await;
        r.db.vcs_insert_object_if_absent(&blob(0xa1), 4)
            .await
            .unwrap();
        let c1 = cs(
            &r,
            &[],
            "a",
            "2026-01-01T00:00:00Z",
            vec![
                fc("p", ChangeKind::Add, Some(blob(0xa1))),
                fc("q", ChangeKind::Add, Some(blob(0xa1))),
            ],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "b",
            "2026-01-01T00:01:00Z",
            vec![
                fc("p", ChangeKind::Modify, Some(blob(0xa1))),
                fc("q", ChangeKind::Modify, Some(blob(0xa1))),
            ],
        )
        .await;
        // base-marking itself exceeds cap 1.
        assert_eq!(
            index
                .diff(&r.repo, c1.as_str(), c2.as_str(), None, 10, None)
                .await,
            Err(IndexError::FrontierTooLarge)
        );
    }

    // ---- blame ----

    #[tokio::test]
    async fn blame_attribution_across_edits() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let v1 = blob(0xa1);
        let v2 = blob(0xa2);
        seed_object(&r, &v1, b"l1\nl2\n").await;
        seed_object(&r, &v2, b"l0\nl1\nl2\n").await;
        let c1 = cs(
            &r,
            &[],
            "add",
            "2026-01-01T00:00:00Z",
            vec![fc("f.txt", ChangeKind::Add, Some(v1.clone()))],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "ins",
            "2026-01-01T00:01:00Z",
            vec![fc("f.txt", ChangeKind::Modify, Some(v2.clone()))],
        )
        .await;
        let (lines, next) = index
            .blame(&r.repo, "f.txt", c2.as_str(), 10, None)
            .await
            .unwrap();
        assert!(next.is_none());
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].changeset_id, c2, "inserted l0 → c2");
        assert_eq!(lines[1].changeset_id, c1, "carried l1 → c1");
        assert_eq!(lines[2].changeset_id, c1, "carried l2 → c1");
        assert_eq!(lines[0].content, b"l0");
        assert_eq!(lines[0].author, "T <t@example.com>");
        // Second call hits the shard cache — same answer.
        let (lines2, _) = index
            .blame(&r.repo, "f.txt", c2.as_str(), 10, None)
            .await
            .unwrap();
        assert_eq!(lines, lines2);
        // blaming at c1 (pre-edit) sees only c1 lines.
        let (lines, _) = index
            .blame(&r.repo, "f.txt", c1.as_str(), 10, None)
            .await
            .unwrap();
        assert_eq!(lines.len(), 2);
        // absent path → PathNotFound.
        assert_eq!(
            index.blame(&r.repo, "nope", c2.as_str(), 10, None).await,
            Err(IndexError::PathNotFound)
        );
        // blame pagination: limit=1 pages through shards.
        let (p1, n1) = index
            .blame(&r.repo, "f.txt", c2.as_str(), 1, None)
            .await
            .unwrap();
        assert_eq!(p1.len(), 1);
        let (p2, n2) = index
            .blame(&r.repo, "f.txt", c2.as_str(), 1, Some(&n1.unwrap()))
            .await
            .unwrap();
        assert_eq!(p2.len(), 1);
        let (p3, n3) = index
            .blame(&r.repo, "f.txt", c2.as_str(), 1, Some(&n2.unwrap()))
            .await
            .unwrap();
        assert_eq!(p3.len(), 1);
        assert!(n3.is_none());
        assert_eq!(p3[0].content, b"l2");
    }

    #[tokio::test]
    async fn blame_rename_chain_and_binary_lines() {
        let (r, index) = rig(ReadIndexCaps::default()).await;
        let v1 = blob(0xa1);
        let v2 = blob(0xa2);
        // Binary lines: NUL + CRLF bytes survive verbatim.
        seed_object(&r, &v1, b"nul\x00line\r\nsecond\n").await;
        seed_object(&r, &v2, b"nul\x00line\r\nsecond\nthird\n").await;
        let c1 = cs(
            &r,
            &[],
            "add",
            "2026-01-01T00:00:00Z",
            vec![fc("old.bin", ChangeKind::Add, Some(v1.clone()))],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "ren+edit",
            "2026-01-01T00:01:00Z",
            vec![FileChangeInput {
                path: "new.bin".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: Some("old.bin".into()),
                blob_hash: Some(v2.clone()),
            }],
        )
        .await;
        let (lines, _) = index
            .blame(&r.repo, "new.bin", c2.as_str(), 10, None)
            .await
            .unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].content, b"nul\x00line\r");
        assert_eq!(
            lines[0].changeset_id, c1,
            "renamed-away lines attribute through the chain"
        );
        assert_eq!(lines[2].changeset_id, c2, "added line → rename changeset");
    }

    #[tokio::test]
    async fn blame_touches_cap_trips() {
        let (r, index) = rig(ReadIndexCaps {
            blame_touches_max: 1,
            ..ReadIndexCaps::default()
        })
        .await;
        let v1 = blob(0xa1);
        let v2 = blob(0xa2);
        seed_object(&r, &v1, b"a\n").await;
        seed_object(&r, &v2, b"a\nb\n").await;
        let c1 = cs(
            &r,
            &[],
            "add",
            "2026-01-01T00:00:00Z",
            vec![fc("f", ChangeKind::Add, Some(v1))],
        )
        .await;
        let c2 = cs(
            &r,
            &[c1.as_str()],
            "mod",
            "2026-01-01T00:01:00Z",
            vec![fc("f", ChangeKind::Modify, Some(v2))],
        )
        .await;
        // 2 touches > cap 1 → FrontierTooLarge.
        assert_eq!(
            index.blame(&r.repo, "f", c2.as_str(), 10, None).await,
            Err(IndexError::FrontierTooLarge)
        );
    }

    #[tokio::test]
    async fn skipped_db_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let objects = ObjectStore::new(Database::skipped(), cfg(dir.path())).unwrap();
        let index = ReadIndex::new(Database::skipped(), objects, ReadIndexCaps::default());
        let cs_id = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR";
        assert_eq!(
            index.log("r", cs_id, 10, None).await,
            Err(IndexError::DbNotConfigured)
        );
        assert_eq!(
            index.merge_base("r", cs_id, cs_id).await,
            Err(IndexError::DbNotConfigured)
        );
        assert_eq!(
            index.tree("r", cs_id, None, 10, None).await,
            Err(IndexError::DbNotConfigured)
        );
        assert_eq!(
            index.file_at("r", cs_id, "f").await,
            Err(IndexError::DbNotConfigured)
        );
        assert_eq!(
            index.path_history("r", "f", 10, None).await,
            Err(IndexError::DbNotConfigured)
        );
        assert_eq!(
            index.diff("r", cs_id, cs_id, None, 10, None).await,
            Err(IndexError::DbNotConfigured)
        );
        assert_eq!(
            index.blame("r", "f", cs_id, 10, None).await,
            Err(IndexError::DbNotConfigured)
        );
    }
}
