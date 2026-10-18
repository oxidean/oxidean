//! Changeset identity + single-transaction creation (u4).
//!
//! [`ChangesetStore::create`] validates the full input fail-closed (no DB
//! calls until every field passes), derives the deterministic
//! `cs_…` id via [`canonical_changeset`], builds the `path_touch` derived
//! rows via [`crate::read_index::ReadIndex::update_for`], and lands the
//! changeset + file changes + object links + index rows in one
//! [`Database::vcs_create_changeset_tx`] transaction.
//!
//! Identity: `sha256(canonical_input)` where `canonical_input` is the compact
//! JSON `{"v":1,"repo","parents","author","message","manifest_hash"}` —
//! `repo` is hash material (global `id` PK requires repo scoping; recorded
//! FD-Q1 deviation), `manifest_hash` is `sha256(manifest_json)` hex where
//! `manifest_json` is `{"changes":[…]}` sorted by byte-order `path` then
//! `old_path` (None first), `parents` dedup keeping first occurrence in
//! GIVEN order (first-parent semantics), and `committed_at` is deliberately
//! excluded. The id packs the first 128 bits MSB-first into 26 uppercase
//! Crockford chars; `idem_key` keeps the full 256-bit digest so the
//! accepted 128-bit prefix-collision tail stays distinguishable.
//! [`canonical_changeset`] is THE identity recipe — u5/u6/u7 call it; prose
//! re-implementation is a defect.

use oxidean_core::vcs_types::ChangesetId;
use oxidean_db::{Database, VcsChangesetInsert, VcsFileChangeInsert};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// The only four facade error strings that decode to typed variants —
/// whole-string equality, never substring/prefix (SR4.2). The facade consts
/// are private to `oxidean-db`; the literals pin here by contract.
const SENTINEL_PARENT: &str = "vcs.not_found:parent";
const SENTINEL_OBJECT: &str = "vcs.not_found:object";
const SENTINEL_PARENTS: &str = "vcs.invalid_input:parents";
const SENTINEL_FILE_CHANGE: &str = "vcs.invalid_input:file_change";

/// Pinned octal mode set (FD entities; corrects the stale `41471` constant —
/// `0o120000` symlink is 40960).
const LEGAL_MODES: [i64; 4] = [33188, 33261, 40960, 16384];
const MAX_PATH_LEN: usize = 512;
const MAX_AUTHOR_LEN: usize = 255;
const MAX_MESSAGE_LEN: usize = 65_535;
const MAX_PARENTS: usize = 64;

/// Crockford Base32 alphabet (uppercase, no I/L/O/U) — 128 bits MSB-first,
/// 26 chars (26*5 = 130 bits; the last char's 5-bit window spans digest
/// bits 125..129 — only 125..127 exist, so 3 significant bits and the
/// alphabet index is one of {0,4,…,28}).
const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Errors surfaced by [`ChangesetStore`]. u7 maps variants to wire codes;
/// the enum never self-stringifies into wire data (LC4.6 shapes:
/// `InvalidInput` carries the rejection reason — never the rejected VALUE;
/// `Internal` carries raw facade/io detail for host diagnostics, opaque
/// to the wire).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChangesetError {
    /// A parent changeset id does not exist (`vcs.not_found:parent`).
    ParentNotFound,
    /// A manifest blob hash has no `vcs_objects` row (`vcs.not_found:object`).
    /// `hash` is populated only by the post-sentinel `vcs_get_object` probe on
    /// the request's candidate hashes; `None` when the probe cannot isolate
    /// the missing hash (SR4.2).
    ObjectNotFound { hash: Option<String> },
    /// `get`/`lineage` on an absent id.
    ChangesetNotFound,
    /// Fail-closed input rejection — `field: reason` text, never echoes
    /// rejected values (paths, modes, messages, authors).
    InvalidInput(String),
    /// Any non-sentinel database/internal failure — raw detail carried for
    /// host diagnostics; opaque to the wire.
    Internal(String),
    /// `Database::is_skipped()` — eager fail-closed check (BR4.16).
    DbNotConfigured,
}

impl std::fmt::Display for ChangesetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangesetError::ParentNotFound => write!(f, "parent changeset not found"),
            ChangesetError::ObjectNotFound { .. } => write!(f, "object not found"),
            ChangesetError::ChangesetNotFound => write!(f, "changeset not found"),
            ChangesetError::InvalidInput(reason) => write!(f, "invalid input: {reason}"),
            ChangesetError::Internal(detail) => write!(f, "internal error: {detail}"),
            ChangesetError::DbNotConfigured => write!(f, "database not configured"),
        }
    }
}

impl std::error::Error for ChangesetError {}

/// Field name folds into the reason text (`"field: reason"`) — the rejected
/// VALUE is never echoed (LC4.6).
fn invalid_input(field: &str, reason: &str) -> ChangesetError {
    warned(ChangesetError::InvalidInput(format!("{field}: {reason}")))
}

/// Raw facade/io detail rides inside `Internal` — opaque to the wire,
/// visible to host diagnostics (LC4.6).
fn internal(detail: String) -> ChangesetError {
    warned(ChangesetError::Internal(detail))
}

/// Every typed-error return warns once, at construction (SD-obs: sentinel
/// decodes included; `error = %e` carries the mapped error only — never a
/// path/message/author value).
fn warned(e: ChangesetError) -> ChangesetError {
    tracing::warn!(error = %e, "vcs changeset typed error");
    e
}

/// A single manifest entry — the create-time and read-back shape.
///
/// `old_path` is present iff `kind == rename`; `blob_hash` is present iff
/// `kind != delete`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeInput {
    pub path: String,
    pub kind: ChangeKind,
    pub mode: i64,
    pub old_path: Option<String>,
    pub blob_hash: Option<String>,
}

/// The four file-change kinds; serializes as lowercase in the canonical
/// manifest recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Add,
    Modify,
    Delete,
    Rename,
}

impl ChangeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Add => "add",
            ChangeKind::Modify => "modify",
            ChangeKind::Delete => "delete",
            ChangeKind::Rename => "rename",
        }
    }

    pub fn parse(s: &str) -> Option<ChangeKind> {
        match s {
            "add" => Some(ChangeKind::Add),
            "modify" => Some(ChangeKind::Modify),
            "delete" => Some(ChangeKind::Delete),
            "rename" => Some(ChangeKind::Rename),
            _ => None,
        }
    }
}

/// Create request — `committed_at: None` stamps `Utc::now()` (TD4.6).
#[derive(Debug, Clone)]
pub struct CreateChangesetInput {
    pub repo_id: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub message: String,
    pub committed_at: Option<String>,
    pub changes: Vec<FileChangeInput>,
}

/// Everything the canonical recipe derives — `manifest_json` is hash material
/// ONLY and is never persisted (no manifest column exists; TD4.5).
#[derive(Debug)]
pub struct CanonicalChangeset {
    /// Canonical manifest JSON — byte-exact hash material only.
    pub manifest_json: String,
    /// sha256 hex of `manifest_json`.
    pub manifest_hash: String,
    /// Canonical input JSON fed to the identity hash.
    pub canonical_input: String,
    /// Full sha256 hex of `canonical_input` — the idempotency key.
    pub idem_key: String,
    /// `cs_` + 26 uppercase Crockford chars of the digest's first 128 bits.
    pub id: ChangesetId,
}

/// A stored changeset as read back through the facade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangesetRecord {
    pub id: ChangesetId,
    pub repo_id: String,
    pub parents: Vec<ChangesetId>,
    pub author: String,
    pub message: String,
    pub committed_at: String,
    pub manifest_hash: String,
    pub manifest: Vec<FileChangeInput>,
}

/// `pub(crate)` id-shape oracle — the public `ChangesetId::parse` wrapped so
/// ref-registry id-shape rejection and read-walk input checks share it.
pub(crate) fn is_changeset_id(s: &str) -> bool {
    ChangesetId::parse(s).is_ok()
}

/// Pure lowercase hex of 40 or 64 chars — the git-oid probe shape (BR4.8) and
/// a ref-name rejection class (BR4.9).
pub(crate) fn is_git_oid(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64)
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// sha256 hex digest of `s`.
fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// Pack the digest's first 128 bits MSB-first into the canonical 26-char
/// uppercase Crockford body (hand-rolled — TD4.4). 26 groups × 5 bits = 130;
/// only the top 3 bits of the final group carry digest data (bits beyond
/// byte 15 read as zero).
fn crockford_26(digest: &[u8]) -> String {
    let mut out = String::with_capacity(26);
    for i in 0..26usize {
        let bit = i * 5;
        let byte = bit / 8;
        let shift = bit % 8;
        // 5-bit window may straddle a byte boundary; bits ≥128 are zero.
        let hi = if byte < 16 { digest[byte] as u16 } else { 0 };
        let lo = if byte + 1 < 16 {
            digest[byte + 1] as u16
        } else {
            0
        };
        let v = ((hi << 8) | lo) >> (11 - shift) & 0x1f;
        out.push(CROCKFORD[v as usize] as char);
    }
    out
}

/// Path grammar (SR4.1): 1..=512 chars, POSIX `/` separators, no NUL, no
/// control bytes (<0x20, 0x7f), no leading/trailing `/`, no `.`/`..`
/// segments, no `//`.
pub(crate) fn validate_path(path: &str) -> Result<(), ChangesetError> {
    // VARCHAR(512) is char-typed — bound by characters, not bytes.
    if path.is_empty() || path.chars().count() > MAX_PATH_LEN {
        return Err(invalid_input("path", "length must be 1..=512"));
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(invalid_input(
            "path",
            "must not have a leading or trailing '/'",
        ));
    }
    for b in path.bytes() {
        if b == 0 || b < 0x20 || b == 0x7f {
            return Err(invalid_input("path", "contains NUL or control byte"));
        }
    }
    if path.contains("//") {
        return Err(invalid_input("path", "empty segment (contains '//')"));
    }
    for seg in path.split('/') {
        if seg == "." || seg == ".." {
            return Err(invalid_input("path", "contains '.' or '..' segment"));
        }
    }
    Ok(())
}

fn is_lower_hex_64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Validate one manifest entry against the SR4.1 grammar (value echoes are
/// deliberately absent from the error strings).
fn validate_change(c: &FileChangeInput) -> Result<(), ChangesetError> {
    validate_path(&c.path)?;
    if !LEGAL_MODES.contains(&c.mode) {
        return Err(invalid_input("mode", "must be one of the pinned mode set"));
    }
    match c.kind {
        ChangeKind::Rename => {
            let Some(old) = c.old_path.as_deref() else {
                return Err(invalid_input("old_path", "required for rename"));
            };
            validate_path(old)?;
            if old == c.path {
                return Err(invalid_input("old_path", "must differ from path"));
            }
        }
        _ => {
            if c.old_path.is_some() {
                return Err(invalid_input("old_path", "present only on rename entries"));
            }
        }
    }
    match c.kind {
        ChangeKind::Delete => {
            if c.blob_hash.is_some() {
                return Err(invalid_input(
                    "blob_hash",
                    "present only on non-delete entries",
                ));
            }
        }
        _ => {
            let Some(h) = c.blob_hash.as_deref() else {
                return Err(invalid_input("blob_hash", "required for non-delete"));
            };
            if !is_lower_hex_64(h) {
                return Err(invalid_input("blob_hash", "must be 64 lowercase hex chars"));
            }
        }
    }
    Ok(())
}

/// Fixed-field serde structs pin the canonical JSON byte shape (serde_json
/// emits declaration order deterministically — TD4.5). Optional fields are
/// `skip_serializing_if` so `None` never lands as `null`.

#[derive(Serialize)]
struct ManifestEntry<'a> {
    path: &'a str,
    kind: &'a str,
    mode: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    blob: Option<&'a str>,
}

/// `manifest_json` is an OBJECT wrapper — `{"changes":[…]}` (FD-Q1/SD4.3),
/// never a bare array.
#[derive(Serialize)]
struct ManifestDoc<'a> {
    changes: Vec<ManifestEntry<'a>>,
}

/// Field order is contractual: `{"v":1,"repo","parents","author","message",
/// "manifest_hash"}` (FD-Q1/BR4.1).
#[derive(Serialize)]
struct CanonicalInput<'a> {
    v: u8,
    repo: &'a str,
    parents: &'a [String],
    author: &'a str,
    message: &'a str,
    manifest_hash: &'a str,
}

/// THE changeset-identity recipe — hash material only. `author` is already
/// composed by validation. `parents` dedup here too, keeping the FIRST
/// occurrence in GIVEN order — `parents[0]` is first-parent semantics
/// consumed by materialize/file_state_at, so sorting is forbidden (the CG
/// plan's `parents(sorted)` wording is a known defect; FD-Q1 wins).
/// Manifest entries stable-sort by byte-order `path` then `old_path`
/// (`None` first — the canonical order the stored path-ordered rows
/// reproduce).
pub fn canonical_changeset(
    repo_id: &str,
    parents: &[String],
    author: &str,
    message: &str,
    changes: &[FileChangeInput],
) -> CanonicalChangeset {
    let mut deduped: Vec<String> = Vec::with_capacity(parents.len());
    for p in parents {
        if !deduped.contains(p) {
            deduped.push(p.clone());
        }
    }
    let mut entries: Vec<ManifestEntry<'_>> = changes
        .iter()
        .map(|c| ManifestEntry {
            path: &c.path,
            kind: c.kind.as_str(),
            mode: c.mode,
            old_path: c.old_path.as_deref(),
            blob: c.blob_hash.as_deref(),
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(b.path).then_with(|| a.old_path.cmp(&b.old_path)));
    // serde_json cannot fail on plain struct serialisation.
    let manifest_json = serde_json::to_string(&ManifestDoc { changes: entries })
        .unwrap_or_else(|_| "{\"changes\":[]}".to_string());
    let manifest_hash = sha256_hex(&manifest_json);
    let canonical_input = serde_json::to_string(&CanonicalInput {
        v: 1,
        repo: repo_id,
        parents: &deduped,
        author,
        message,
        manifest_hash: &manifest_hash,
    })
    .unwrap_or_default();
    let digest = Sha256::digest(canonical_input.as_bytes());
    let idem_key = hex::encode(digest);
    let id = ChangesetId::parse(&format!("cs_{}", crockford_26(&digest)))
        .expect("cs_ + 26 crockford chars always parses");
    CanonicalChangeset {
        manifest_json,
        manifest_hash,
        canonical_input,
        idem_key,
        id,
    }
}

/// Normalize a committed_at input to `%Y-%m-%dT%H:%M:%SZ` UTC (SR4.1).
/// RFC-3339 offsets are converted; fractional seconds and out-of-DATETIME-
/// range values are rejected.
fn normalize_committed_at(raw: Option<&str>) -> Result<String, ChangesetError> {
    use chrono::{DateTime, Datelike, SecondsFormat, Utc};
    let dt: DateTime<Utc> = match raw {
        None => Utc::now(),
        Some(s) => {
            if s.contains('.') {
                return Err(invalid_input(
                    "committed_at",
                    "fractional seconds are not permitted",
                ));
            }
            let parsed = DateTime::parse_from_rfc3339(s)
                .map_err(|_| invalid_input("committed_at", "must be an RFC-3339 timestamp"))?;
            let dt = parsed.with_timezone(&Utc);
            if dt.year() < 1000 || dt.year() > 9999 {
                return Err(invalid_input("committed_at", "outside the DATETIME range"));
            }
            dt
        }
    };
    Ok(dt.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// Compose the canonical `Name <email>` author string (SR4.1). Name is
/// trimmed; composed length ≤255; neither part may carry `<`, `>`, newline,
/// or control bytes.
fn compose_author(name: &str, email: &str) -> Result<String, ChangesetError> {
    let name = name.trim();
    if name.is_empty() || email.is_empty() {
        return Err(invalid_input("author", "name and email are required"));
    }
    // Angle brackets live only in the ` <…>` separator — reject them inside
    // the parts so the composed shape stays canonical `Name <email>`.
    for part in [name, email] {
        for b in part.bytes() {
            if b == b'<' || b == b'>' || b == b'\n' || b == b'\r' || b < 0x20 || b == 0x7f {
                return Err(invalid_input(
                    "author",
                    "contains '<', '>', newline, or control byte",
                ));
            }
        }
    }
    let composed = format!("{name} <{email}>");
    if composed.chars().count() > MAX_AUTHOR_LEN {
        return Err(invalid_input("author", "exceeds 255 characters"));
    }
    Ok(composed)
}

/// Full input validation — every check lands before any DB call (SR4.1).
/// Returns the deduped parents, composed author, normalized committed_at, and
/// the validated changes (unchanged shape).
fn validate_input(
    input: &CreateChangesetInput,
) -> Result<(Vec<String>, String, String), ChangesetError> {
    if input.parents.len() > MAX_PARENTS {
        return Err(invalid_input("parents", "at most 64 parents"));
    }
    let mut deduped: Vec<String> = Vec::with_capacity(input.parents.len());
    for p in &input.parents {
        if !is_changeset_id(p) {
            return Err(invalid_input("parents", "entries must be changeset ids"));
        }
        if !deduped.contains(p) {
            deduped.push(p.clone());
        }
    }
    if input.message.len() > MAX_MESSAGE_LEN {
        return Err(invalid_input("message", "exceeds 65535 bytes"));
    }
    let committed_at = normalize_committed_at(input.committed_at.as_deref())?;
    let author = compose_author(&input.author_name, &input.author_email)?;

    let mut seen_paths = std::collections::HashSet::with_capacity(input.changes.len());
    let mut seen_old = std::collections::HashSet::with_capacity(input.changes.len());
    for c in &input.changes {
        validate_change(c)?;
        if !seen_paths.insert(&c.path) {
            return Err(invalid_input("path", "duplicate path in manifest"));
        }
        if let Some(old) = c.old_path.as_ref() {
            if !seen_old.insert(old) {
                return Err(invalid_input(
                    "old_path",
                    "a path may be the source of at most one rename",
                ));
            }
        }
    }
    Ok((deduped, author, committed_at))
}

/// Decode a `vcs_create_changeset_tx` error: whole-string sentinel equality
/// only — anything else is `Internal` carrying the RAW facade text for host
/// diagnostics (SR4.2/LC4.6; opaque to the wire).
fn decode_create_error(err: &str) -> ChangesetError {
    match err {
        SENTINEL_PARENT => ChangesetError::ParentNotFound,
        SENTINEL_OBJECT => ChangesetError::ObjectNotFound { hash: None },
        SENTINEL_PARENTS | SENTINEL_FILE_CHANGE => {
            ChangesetError::InvalidInput("manifest: rejected by the store".to_string())
        }
        _ => ChangesetError::Internal(err.to_string()),
    }
}

/// Changeset store — creation, fetch, lineage, and the git-oid import probe.
pub struct ChangesetStore {
    db: Database,
}

impl ChangesetStore {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Validate → canonical identity → idempotency probe → single
    /// `vcs_create_changeset_tx` transaction (WF1/R-01).
    pub async fn create(&self, input: CreateChangesetInput) -> Result<ChangesetId, ChangesetError> {
        if self.db.is_skipped() {
            return Err(warned(ChangesetError::DbNotConfigured));
        }
        let (parents, author, committed_at) = validate_input(&input)?;
        let canon = canonical_changeset(
            &input.repo_id,
            &parents,
            &author,
            &input.message,
            &input.changes,
        );

        // Idempotent resubmit probe — same idem_key returns the existing id
        // without writing (the tx repeats this check internally for races).
        match self
            .db
            .vcs_find_changeset_by_idem(&input.repo_id, &canon.idem_key)
            .await
        {
            Ok(Some(row)) => {
                return ChangesetId::parse(&row.id).map_err(internal);
            }
            Ok(None) => {}
            Err(e) => return Err(warned(decode_create_error(&e))),
        }

        // path_touch derived rows — owned strings, then borrowed facade rows.
        let rows = crate::read_index::ReadIndex::update_for(
            canon.id.as_str(),
            &committed_at,
            &input.changes,
        );
        let upserts: Vec<oxidean_db::VcsDerivedIndexUpsert<'_>> = rows
            .iter()
            .map(|r| oxidean_db::VcsDerivedIndexUpsert {
                kind: r.kind,
                index_key: &r.index_key,
                payload: &r.payload,
                covering_changeset: &r.covering_changeset,
            })
            .collect();

        let file_changes: Vec<VcsFileChangeInsert<'_>> = input
            .changes
            .iter()
            .map(|c| VcsFileChangeInsert {
                path: &c.path,
                kind: c.kind.as_str(),
                mode: c.mode,
                old_path: c.old_path.as_deref(),
                blob_hash: c.blob_hash.as_deref(),
            })
            .collect();

        // Object links — every manifest blob, deduped (AC3.5.2).
        let mut links: Vec<&str> = Vec::with_capacity(input.changes.len());
        for c in &input.changes {
            if let Some(h) = c.blob_hash.as_deref() {
                if !links.contains(&h) {
                    links.push(h);
                }
            }
        }

        let parents_json = serde_json::to_string(&parents).unwrap_or_default();
        let insert = VcsChangesetInsert {
            id: canon.id.as_str(),
            repo_id: &input.repo_id,
            parents: &parents_json,
            author: &author,
            message: &input.message,
            committed_at: &committed_at,
            manifest_hash: &canon.manifest_hash,
            idem_key: Some(canon.idem_key.as_str()),
        };
        let index_rows = upserts.len();
        match self
            .db
            .vcs_create_changeset_tx(&insert, &file_changes, &links, &upserts)
            .await
        {
            Ok(id) => {
                tracing::info!(
                    event = "vcs.index.upsert",
                    rows = index_rows,
                    "path_touch rows written"
                );
                ChangesetId::parse(&id).map_err(internal)
            }
            Err(e) if e == SENTINEL_OBJECT => {
                // Pinned post-sentinel probe: isolate the missing hash
                // (`vcs_get_object` is global — objects are content-
                // addressed, not repo-scoped).
                for h in &links {
                    match self.db.vcs_get_object(h).await {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            return Err(warned(ChangesetError::ObjectNotFound {
                                hash: Some((*h).to_string()),
                            }));
                        }
                        Err(_) => break,
                    }
                }
                Err(warned(ChangesetError::ObjectNotFound { hash: None }))
            }
            // One warn per typed-error return — sentinel decodes included
            // (SD-obs; no double warn — decode_create_error stays pure).
            Err(e) => Err(warned(decode_create_error(&e))),
        }
    }

    /// Fetch a changeset + manifest. Malformed id → `InvalidInput`, absent →
    /// `ChangesetNotFound`.
    pub async fn get(&self, repo_id: &str, id: &str) -> Result<ChangesetRecord, ChangesetError> {
        if self.db.is_skipped() {
            return Err(warned(ChangesetError::DbNotConfigured));
        }
        if !is_changeset_id(id) {
            return Err(invalid_input("id", "not a changeset id"));
        }
        let row = self
            .db
            .vcs_get_changeset(repo_id, id)
            .await
            .map_err(|e| warned(decode_create_error(&e)))?
            .ok_or_else(|| warned(ChangesetError::ChangesetNotFound))?;
        let parents = decode_parents(&row.parents)?;
        let manifest = self.manifest_of(id).await?;
        Ok(ChangesetRecord {
            id: ChangesetId::parse(&row.id).map_err(internal)?,
            repo_id: row.repo_id,
            parents,
            author: row.author,
            message: row.message,
            committed_at: row.committed_at,
            manifest_hash: row.manifest_hash,
            manifest,
        })
    }

    /// Stored parent ids in manifest order. Absent → `ChangesetNotFound`.
    pub async fn lineage(
        &self,
        repo_id: &str,
        id: &str,
    ) -> Result<Vec<ChangesetId>, ChangesetError> {
        if self.db.is_skipped() {
            return Err(warned(ChangesetError::DbNotConfigured));
        }
        if !is_changeset_id(id) {
            return Err(invalid_input("id", "not a changeset id"));
        }
        let row = self
            .db
            .vcs_get_changeset(repo_id, id)
            .await
            .map_err(|e| warned(decode_create_error(&e)))?
            .ok_or_else(|| warned(ChangesetError::ChangesetNotFound))?;
        decode_parents(&row.parents)
    }

    async fn manifest_of(&self, id: &str) -> Result<Vec<FileChangeInput>, ChangesetError> {
        let rows = self
            .db
            .vcs_list_file_changes(id)
            .await
            .map_err(|e| warned(decode_create_error(&e)))?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let kind = ChangeKind::parse(&r.kind)
                .ok_or_else(|| internal(format!("stored file_change kind: {}", r.kind)))?;
            out.push(FileChangeInput {
                path: r.path,
                kind,
                mode: r.mode,
                old_path: r.old_path,
                blob_hash: r.blob_hash,
            });
        }
        Ok(out)
    }

    /// Git-OID import probe (BR4.8): a 40/64-hex arg scans
    /// `vcs_import_links` for `{source:"git", external_id}`. Zero indexed rows
    /// → `Ok(None)`; malformed arg → `InvalidInput`.
    pub async fn resolve_git_oid(
        &self,
        repo_id: &str,
        git_oid: &str,
    ) -> Result<Option<ChangesetId>, ChangesetError> {
        if self.db.is_skipped() {
            return Err(warned(ChangesetError::DbNotConfigured));
        }
        if !is_git_oid(git_oid) {
            return Err(invalid_input("git_oid", "must be 40 or 64 lowercase hex"));
        }
        let hit = self
            .db
            .vcs_lookup_import_link(repo_id, git_oid)
            .await
            .map_err(internal)?;
        hit.map(|id| ChangesetId::parse(&id).map_err(internal))
            .transpose()
    }
}

fn decode_parents(parents_json: &str) -> Result<Vec<ChangesetId>, ChangesetError> {
    let raw: Vec<String> = serde_json::from_str(parents_json)
        .map_err(|e| internal(format!("stored parents JSON: {e}")))?;
    raw.iter()
        .map(|p| ChangesetId::parse(p).map_err(internal))
        .collect()
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::object_store::testutil::{seed_repo, test_db};

    fn blob(byte: u8) -> String {
        format!("{:02x}", byte).repeat(32)
    }

    fn change(path: &str, kind: ChangeKind, blob: Option<String>) -> FileChangeInput {
        FileChangeInput {
            path: path.to_string(),
            kind,
            mode: 33188,
            old_path: None,
            blob_hash: blob,
        }
    }

    fn input(repo: &str) -> CreateChangesetInput {
        CreateChangesetInput {
            repo_id: repo.to_string(),
            parents: vec![],
            author_name: "T One".to_string(),
            author_email: "tone@example.com".to_string(),
            message: "hello".to_string(),
            committed_at: Some("2026-01-02T03:04:05Z".to_string()),
            changes: vec![change("src/a.rs", ChangeKind::Add, Some(blob(0xa1)))],
        }
    }

    const PARENT_A: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR";

    // ---- canonical identity (golden vectors) ----

    #[test]
    fn canonical_golden_bytes() {
        let parents = vec![PARENT_A.to_string()];
        let changes = vec![change("src/a.rs", ChangeKind::Add, Some(blob(0xa1)))];
        let c = canonical_changeset(
            "repo-1",
            &parents,
            "T One <tone@example.com>",
            "hello",
            &changes,
        );
        // {"changes":[…]} object wrapper — never a bare array (FD-Q1/SD4.3).
        assert_eq!(
            c.manifest_json,
            format!(
                "{{\"changes\":[{{\"path\":\"src/a.rs\",\"kind\":\"add\",\"mode\":33188,\"blob\":\"{}\"}}]}}",
                blob(0xa1)
            ),
            "manifest recipe drifted"
        );
        assert_eq!(
            c.manifest_hash,
            sha256_hex(&c.manifest_json),
            "manifest_hash must be sha256(manifest_json)"
        );
        // Pinned field order {"v":1,"repo","parents","author","message",
        // "manifest_hash"} (FD-Q1/BR4.1).
        assert_eq!(
            c.canonical_input,
            format!(
                "{{\"v\":1,\"repo\":\"repo-1\",\"parents\":[\"{PARENT_A}\"],\"author\":\"T One <tone@example.com>\",\"message\":\"hello\",\"manifest_hash\":\"{}\"}}",
                c.manifest_hash
            ),
            "canonical input recipe drifted"
        );
        // Independently recomputed over the pinned bytes (sha256sum):
        assert_eq!(
            c.idem_key,
            "233037a4214a6e8872c67919f5dc5731d1cc34c3bab86af965676ea2c4ea6f19"
        );
        assert_eq!(c.idem_key, sha256_hex(&c.canonical_input));
        assert!(
            is_changeset_id(c.id.as_str()),
            "id shape: {}",
            c.id.as_str()
        );
        assert_eq!(c.id.as_str().len(), 29);
    }

    #[test]
    fn canonical_manifest_sorted_and_parents_order_preserved() {
        // Caller order is irrelevant — canonical entries sort by byte-order
        // path, then old_path (None first).
        let changes = vec![
            change("b/y", ChangeKind::Add, Some(blob(0xc3))),
            FileChangeInput {
                path: "a/z".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: Some("a/old".into()),
                blob_hash: Some(blob(0xd4)),
            },
            change("a/x", ChangeKind::Modify, Some(blob(0xb2))),
        ];
        let c = canonical_changeset("r", &[], "A <a@b>", "m", &changes);
        let a_x = c.manifest_json.find("\"a/x\"").expect("a/x");
        let a_z = c.manifest_json.find("\"a/z\"").expect("a/z");
        let b_y = c.manifest_json.find("\"b/y\"").expect("b/y");
        assert!(a_x < a_z && a_z < b_y, "manifest must sort by path");
        // The same manifest in a different caller order → identical bytes.
        let mut reversed = changes.clone();
        reversed.reverse();
        let c2 = canonical_changeset("r", &[], "A <a@b>", "m", &reversed);
        assert_eq!(c.manifest_json, c2.manifest_json);
        assert_eq!(c.id, c2.id, "entry order must not change identity");
        // Parents dedup keeps FIRST occurrence in given order — never sorted
        // (parents[0] is first-parent semantics).
        let pb = "cs_02J9W5MHZ1M3C0R8Y6G7N8PQST".to_string();
        let pa = PARENT_A.to_string();
        let c = canonical_changeset(
            "r",
            &[pb.clone(), pa.clone(), pb.clone()],
            "A <a@b>",
            "m",
            &changes,
        );
        assert_eq!(
            c.canonical_input,
            format!(
                "{{\"v\":1,\"repo\":\"r\",\"parents\":[\"{pb}\",\"{pa}\"],\"author\":\"A <a@b>\",\"message\":\"m\",\"manifest_hash\":\"{}\"}}",
                c.manifest_hash
            ),
            "parents must be deduped order-preserved, not sorted"
        );
    }

    #[test]
    fn canonical_committed_at_and_rename_fields() {
        // committed_at is not a canonical input — covered by recipe shape
        // above. Here: rename emits old_path and the r-key discriminator is
        // exercised by update_for (read_index tests own that).
        let changes = vec![FileChangeInput {
            path: "b/new.rs".into(),
            kind: ChangeKind::Rename,
            mode: 33188,
            old_path: Some("a/old.rs".into()),
            blob_hash: Some(blob(0xb2)),
        }];
        let c = canonical_changeset("r", &[], "A <a@b>", "m", &changes);
        assert!(c.manifest_json.contains("\"old_path\":\"a/old.rs\""));
        assert!(!c.canonical_input.contains("committed_at"));
    }

    #[test]
    fn canonical_determinism_and_repo_scoping() {
        let changes = vec![change("f", ChangeKind::Add, Some(blob(0x01)))];
        let a = canonical_changeset("repo-1", &[], "A <a@b>", "m", &changes);
        let again = canonical_changeset("repo-1", &[], "A <a@b>", "m", &changes);
        let other_repo = canonical_changeset("repo-2", &[], "A <a@b>", "m", &changes);
        assert_eq!(a.id, again.id, "identity must be deterministic");
        assert_ne!(a.id, other_repo.id, "repo is hash material");
        assert_ne!(a.idem_key, other_repo.idem_key);
    }

    #[test]
    fn crockford_packing_round_trip() {
        // All-zero digest → all-zero body; all-0xFF → alphabet tail chars.
        let zero = crockford_26(&[0u8; 32]);
        assert_eq!(zero, "0".repeat(26));
        let ones = crockford_26(&[0xffu8; 32]);
        assert_eq!(ones.len(), 26);
        assert!(is_changeset_id(&format!("cs_{ones}")));
        // The final char carries only the top 3 bits of group 26 — bits
        // ≥128 are padding (0xFF digest → 0b111_00 = 28 → 'W').
        assert_eq!(ones.chars().nth(25), Some('W'));
        assert_eq!(&ones[..25], &"Z".repeat(25));
        // Spot-check a known input bit pattern: 0x08 → top 5 bits = 00001.
        let mut digest = [0u8; 32];
        digest[0] = 0x08;
        assert!(crockford_26(&digest).starts_with('1'));
    }

    // ---- validation (every arm before any DB call) ----

    #[tokio::test]
    async fn validation_path_grammar() {
        let (_dir, db) = test_db().await;
        let store = ChangesetStore::new(db);
        let bad_paths = [
            "",
            "/abs",
            "trail/",
            "a//b",
            "a/./b",
            "a/../b",
            "a\0b",
            "a\x01b",
            "a\x7fb",
            &"x".repeat(513),
        ];
        for p in bad_paths {
            let mut i = input("r");
            i.changes = vec![change(p, ChangeKind::Add, Some(blob(0xa1)))];
            match store.create(i).await {
                Err(ChangesetError::InvalidInput(reason)) => {
                    assert!(reason.starts_with("path:"), "{reason}");
                    assert!(!reason.contains('x') || p.len() != 513);
                }
                other => panic!("path {p:?} must reject, got {other:?}"),
            }
        }
        // Boundary: exactly 512 chars passes the grammar and proceeds to the
        // DB path — the unseeded blob hashes to ObjectNotFound, proving the
        // path was never rejected.
        let mut i = input("r");
        i.changes = vec![change(&"x".repeat(512), ChangeKind::Add, Some(blob(0xa1)))];
        assert_eq!(
            store.create(i).await.unwrap_err(),
            ChangesetError::ObjectNotFound {
                hash: Some(blob(0xa1))
            }
        );
    }

    #[tokio::test]
    async fn validation_mode_set_and_rename_rules() {
        let (_dir, db) = test_db().await;
        let store = ChangesetStore::new(db);
        // Stale constant 41471 must reject; the pinned set is {33188, 33261,
        // 40960, 16384}.
        for mode in [0, 33188 - 1, 41471, 33260, 12345] {
            let mut i = input("r");
            i.changes = vec![FileChangeInput {
                path: "f".into(),
                kind: ChangeKind::Add,
                mode,
                old_path: None,
                blob_hash: Some(blob(0xa1)),
            }];
            match store.create(i).await {
                Err(ChangesetError::InvalidInput(reason)) => {
                    assert!(reason.starts_with("mode:"), "{reason}");
                    assert!(!reason.contains(&mode.to_string()), "echoed mode value");
                }
                other => panic!("mode {mode} must reject, got {other:?}"),
            }
        }
        // rename without old_path / rename with old_path == path / old_path
        // present on non-rename.
        let cases = [
            FileChangeInput {
                path: "b".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: None,
                blob_hash: Some(blob(0xa1)),
            },
            FileChangeInput {
                path: "same".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: Some("same".into()),
                blob_hash: Some(blob(0xa1)),
            },
            FileChangeInput {
                path: "f".into(),
                kind: ChangeKind::Add,
                mode: 33188,
                old_path: Some("g".into()),
                blob_hash: Some(blob(0xa1)),
            },
        ];
        for c in cases {
            let mut i = input("r");
            i.changes = vec![c];
            assert!(
                matches!(store.create(i).await, Err(ChangesetError::InvalidInput(_))),
                "rename/old_path arm must reject"
            );
        }
        // delete with blob / non-delete without blob.
        for c in [
            FileChangeInput {
                path: "f".into(),
                kind: ChangeKind::Delete,
                mode: 33188,
                old_path: None,
                blob_hash: Some(blob(0xa1)),
            },
            FileChangeInput {
                path: "f".into(),
                kind: ChangeKind::Modify,
                mode: 33188,
                old_path: None,
                blob_hash: None,
            },
            FileChangeInput {
                path: "f".into(),
                kind: ChangeKind::Add,
                mode: 33188,
                old_path: None,
                blob_hash: Some("ZZ".repeat(32)),
            },
        ] {
            let mut i = input("r");
            i.changes = vec![c];
            assert!(matches!(
                store.create(i).await,
                Err(ChangesetError::InvalidInput(_))
            ));
        }
    }

    #[tokio::test]
    async fn validation_manifest_uniqueness() {
        let (_dir, db) = test_db().await;
        let store = ChangesetStore::new(db);
        // duplicate path
        let mut i = input("r");
        i.changes = vec![
            change("dup", ChangeKind::Add, Some(blob(0xa1))),
            change("dup", ChangeKind::Modify, Some(blob(0xa2))),
        ];
        match store.create(i).await {
            Err(ChangesetError::InvalidInput(r)) => {
                assert!(r.starts_with("path:"), "{r}")
            }
            other => panic!("dup path must reject: {other:?}"),
        }
        // duplicate old_path across two renames
        let mut i = input("r");
        i.changes = vec![
            FileChangeInput {
                path: "b".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: Some("a".into()),
                blob_hash: Some(blob(0xa1)),
            },
            FileChangeInput {
                path: "c".into(),
                kind: ChangeKind::Rename,
                mode: 33188,
                old_path: Some("a".into()),
                blob_hash: Some(blob(0xa2)),
            },
        ];
        match store.create(i).await {
            Err(ChangesetError::InvalidInput(r)) => {
                assert!(r.starts_with("old_path:"), "{r}")
            }
            other => panic!("dup old_path must reject: {other:?}"),
        }
    }

    #[tokio::test]
    async fn validation_parents_author_message_committed_at() {
        let (_dir, db) = test_db().await;
        let store = ChangesetStore::new(db);
        // >64 parents
        let mut i = input("r");
        i.parents = (0..65).map(|n| format!("cs_1{n:025X}")).collect();
        // The generated strings may not be valid ids — the arity check fires
        // first only if all are changeset-shaped; assert InvalidInput either way.
        assert!(matches!(
            store.create(i).await,
            Err(ChangesetError::InvalidInput(_))
        ));
        // non-changeset-shaped parent
        let mut i = input("r");
        i.parents = vec!["main".to_string()];
        assert!(matches!(
            store.create(i).await,
            Err(ChangesetError::InvalidInput(r)) if r.starts_with("parents:")
        ));
        // author arms — and no value echo.
        for (name, email) in [
            ("", "a@b.c"),
            ("N", ""),
            ("Bad<Name", "a@b.c"),
            ("N", "bad>mail"),
            ("Line\nBreak", "a@b.c"),
            ("N", "bad\x07mail"),
            (&"n".repeat(300), "a@b.c"),
        ] {
            let mut i = input("r");
            i.author_name = name.to_string();
            i.author_email = email.to_string();
            match store.create(i).await {
                Err(ChangesetError::InvalidInput(reason)) => {
                    assert!(reason.starts_with("author:"), "{reason}");
                    assert!(!reason.contains('@'), "echoed email");
                }
                other => panic!("author ({name:?},{email:?}) must reject: {other:?}"),
            }
        }
        // message bound (bytes).
        let mut i = input("r");
        i.message = "m".repeat(65_536);
        assert!(matches!(
            store.create(i).await,
            Err(ChangesetError::InvalidInput(r)) if r.starts_with("message:")
        ));
        // committed_at: fractions + malformed + range.
        for ts in ["2026-01-02T03:04:05.5Z", "garbage", "2026-13-40T99:99:99Z"] {
            let mut i = input("r");
            i.committed_at = Some(ts.to_string());
            assert!(matches!(
                store.create(i).await,
                Err(ChangesetError::InvalidInput(r)) if r.starts_with("committed_at:")
            ));
        }
        // offsets normalize to Z.
        let n = normalize_committed_at(Some("2026-01-02T05:04:05+02:00")).unwrap();
        assert_eq!(n, "2026-01-02T03:04:05Z");
        // None stamps now, still canonical shape.
        let n = normalize_committed_at(None).unwrap();
        assert_eq!(n.len(), 20);
        assert!(n.ends_with('Z'));
    }

    #[tokio::test]
    async fn validation_precedes_any_db_call() {
        // A skipped DB + invalid input must surface InvalidInput-adjacent
        // ordering per fail-closed rule: is_skipped wins (BR4.16) — every
        // entry checks it first.
        let store = ChangesetStore::new(Database::skipped());
        let mut i = input("r");
        i.changes = vec![change("", ChangeKind::Add, None)];
        assert_eq!(store.create(i).await, Err(ChangesetError::DbNotConfigured));
    }

    // ---- sentinel decode ----

    #[test]
    fn sentinel_exact_decode() {
        assert_eq!(
            decode_create_error("vcs.not_found:parent"),
            ChangesetError::ParentNotFound
        );
        assert_eq!(
            decode_create_error("vcs.not_found:object"),
            ChangesetError::ObjectNotFound { hash: None }
        );
        assert!(matches!(
            decode_create_error("vcs.invalid_input:parents"),
            ChangesetError::InvalidInput(_)
        ));
        assert!(matches!(
            decode_create_error("vcs.invalid_input:file_change"),
            ChangesetError::InvalidInput(_)
        ));
        // Prefix/suffix near-misses are opaque Internal — never typed.
        for near in [
            "vcs.not_found:parentXYZ",
            "x vcs.not_found:parent",
            "vcs.not_found:paren",
            "vcs.not_found:object trailing",
            "vcs cas_ref create: duplicate",
            "some other db error",
            "",
        ] {
            assert!(
                matches!(
                    decode_create_error(near),
                    ChangesetError::Internal(d) if d == *near
                ),
                "near-sentinel {near:?} must be Internal carrying raw detail"
            );
        }
    }

    // ---- live create paths (sqlite fixtures, no mocks) ----

    #[tokio::test]
    async fn create_round_trip_and_links() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-cs", "r-cs").await;
        // Seed both manifest objects (complete link set — AC3.5.2).
        db.vcs_insert_object_if_absent(&blob(0xa1), 4)
            .await
            .unwrap();
        db.vcs_insert_object_if_absent(&blob(0xa2), 4)
            .await
            .unwrap();
        let store = ChangesetStore::new(db.clone());
        let mut i = input(&repo);
        i.changes = vec![
            change("src/a.rs", ChangeKind::Add, Some(blob(0xa1))),
            FileChangeInput {
                path: "src/b.rs".into(),
                kind: ChangeKind::Rename,
                mode: 33261,
                old_path: Some("src/old.rs".into()),
                blob_hash: Some(blob(0xa2)),
            },
            change("gone.rs", ChangeKind::Delete, None),
        ];
        let id = store.create(i.clone()).await.expect("create");
        let rec = store.get(&repo, id.as_str()).await.expect("get");
        assert_eq!(rec.id, id);
        assert_eq!(rec.author, "T One <tone@example.com>");
        assert_eq!(rec.committed_at, "2026-01-02T03:04:05Z");
        assert_eq!(rec.manifest.len(), 3);
        // vcs_list_file_changes returns path-ordered rows.
        let rename = rec
            .manifest
            .iter()
            .find(|c| c.path == "src/b.rs")
            .expect("rename row");
        assert_eq!(rename.old_path.as_deref(), Some("src/old.rs"));
        assert_eq!(rename.kind, ChangeKind::Rename);
        // Every manifest blob is object-linked.
        for c in &rec.manifest {
            if let Some(h) = &c.blob_hash {
                assert!(
                    db.vcs_get_object_link(&repo, h).await.unwrap(),
                    "blob link missing"
                );
            }
        }
        // Idempotent resubmit: same input → same id, still one row.
        let again = store.create(i).await.expect("idempotent create");
        assert_eq!(again, id);
        assert_eq!(db.vcs_count_changesets(&repo).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn concurrent_identical_creates_one_row() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-cc", "r-cc").await;
        db.vcs_insert_object_if_absent(&blob(0xa1), 4)
            .await
            .unwrap();
        let store = ChangesetStore::new(db.clone());
        let i1 = input(&repo);
        let i2 = input(&repo);
        let (r1, r2) = tokio::join!(store.create(i1), store.create(i2));
        assert_eq!(r1.unwrap(), r2.unwrap());
        assert_eq!(db.vcs_count_changesets(&repo).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn divergent_same_parent_creates_distinct_ids() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-div", "r-div").await;
        db.vcs_insert_object_if_absent(&blob(0xa1), 4)
            .await
            .unwrap();
        db.vcs_insert_object_if_absent(&blob(0xa2), 4)
            .await
            .unwrap();
        let store = ChangesetStore::new(db.clone());
        let root = store.create(input(&repo)).await.expect("root");
        let mut i1 = input(&repo);
        i1.parents = vec![root.as_str().to_string()];
        i1.message = "branch one".into();
        let mut i2 = input(&repo);
        i2.parents = vec![root.as_str().to_string()];
        i2.message = "branch two".into();
        i2.changes = vec![change("src/c.rs", ChangeKind::Add, Some(blob(0xa2)))];
        let (c1, c2) = tokio::join!(store.create(i1), store.create(i2));
        let (c1, c2) = (c1.unwrap(), c2.unwrap());
        assert_ne!(c1, c2, "divergent siblings must get distinct ids");
        assert_eq!(db.vcs_count_changesets(&repo).await.unwrap(), 3);
        // Lineage: both descend from root.
        assert_eq!(
            store.lineage(&repo, c1.as_str()).await.unwrap(),
            vec![root.clone()]
        );
        assert_eq!(store.lineage(&repo, c2.as_str()).await.unwrap(), vec![root]);
    }

    #[tokio::test]
    async fn missing_parent_and_missing_object_typed() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-mp", "r-mp").await;
        db.vcs_insert_object_if_absent(&blob(0xa1), 4)
            .await
            .unwrap();
        let store = ChangesetStore::new(db.clone());
        // Parent is id-shaped but absent.
        let mut i = input(&repo);
        i.parents = vec![PARENT_A.to_string()];
        assert_eq!(store.create(i).await, Err(ChangesetError::ParentNotFound));
        // Object absent → ObjectNotFound with the pinned post-sentinel hash.
        let mut i = input(&repo);
        i.changes = vec![change("f", ChangeKind::Add, Some(blob(0xee)))];
        assert_eq!(
            store.create(i).await,
            Err(ChangesetError::ObjectNotFound {
                hash: Some(blob(0xee))
            })
        );
    }

    #[tokio::test]
    async fn skipped_db_fail_closed() {
        let store = ChangesetStore::new(Database::skipped());
        assert_eq!(
            store.create(input("r")).await,
            Err(ChangesetError::DbNotConfigured)
        );
        assert_eq!(
            store.get("r", PARENT_A).await,
            Err(ChangesetError::DbNotConfigured)
        );
        assert_eq!(
            store.lineage("r", PARENT_A).await,
            Err(ChangesetError::DbNotConfigured)
        );
        assert_eq!(
            store.resolve_git_oid("r", &"ab".repeat(20)).await,
            Err(ChangesetError::DbNotConfigured)
        );
    }

    #[tokio::test]
    async fn get_and_git_oid_probe() {
        let (_dir, db) = test_db().await;
        let repo = seed_repo(&db, "u-g", "r-g").await;
        db.vcs_insert_object_if_absent(&blob(0xa1), 4)
            .await
            .unwrap();
        let store = ChangesetStore::new(db.clone());
        let id = store.create(input(&repo)).await.unwrap();
        // missing → ChangesetNotFound; malformed → InvalidInput.
        assert_eq!(
            store.get(&repo, PARENT_A).await,
            Err(ChangesetError::ChangesetNotFound)
        );
        assert!(matches!(
            store.get(&repo, "not-an-id").await,
            Err(ChangesetError::InvalidInput(_))
        ));
        // import-link probe: none indexed → None; malformed oid → InvalidInput.
        assert_eq!(
            store
                .resolve_git_oid(&repo, &"ab".repeat(20))
                .await
                .unwrap(),
            None
        );
        assert!(matches!(
            store.resolve_git_oid(&repo, "XYZ").await,
            Err(ChangesetError::InvalidInput(_))
        ));
        // Land an import link via the facade and resolve it.
        let oid = "cd".repeat(20);
        assert!(db
            .vcs_insert_import_link_if_absent(&repo, &oid, id.as_str())
            .await
            .unwrap());
        assert_eq!(store.resolve_git_oid(&repo, &oid).await.unwrap(), Some(id));
    }
}
