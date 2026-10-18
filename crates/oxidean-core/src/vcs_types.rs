//! Agentic VCS protocol wire types (contract C1).
//!
//! Every `vcs.*` procedure carries `{owner, name}` as separate required string
//! fields — the `{"repo":"o/n"}` composite never exists on this wire (BR1.4).
//! Optional fields serialize absent-not-null (`Option` + `skip_serializing_if`);
//! the two exceptions are the required-nullable CAS fields on `vcs.ref_update`
//! ([`Nullable`]) and `content_b64`/`changeset_id` response nulls, which are
//! meaningful `null`s the server always emits (BR1.5).
//!
//! Wire *shape* only: bound checks (`limit` ≤500, `changes` ≤10_000, name
//! validation) live in the handlers, not the DTOs (SR4). No
//! `deny_unknown_fields` anywhere — additive evolution depends on consumers
//! ignoring unknown fields (BR1.6).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::protection_types::ProtectionBlockReasons;
use crate::AppError;

// ---------------------------------------------------------------------------
// Procedure name set (BR1.2 — exactly these 17 names; nothing else invents
// `vcs.*` procedures).
// ---------------------------------------------------------------------------

/// The pinned `vcs.*` procedure set — 17 names, no more (BR1.2).
pub const VCS_PROCEDURES: &[&str] = &[
    "vcs.repo_enable",
    "vcs.status",
    "vcs.changeset_create",
    "vcs.changeset_get",
    "vcs.log",
    "vcs.diff",
    "vcs.tree",
    "vcs.file_get",
    "vcs.blame",
    "vcs.path_history",
    "vcs.merge_base",
    "vcs.merge",
    "vcs.rebase",
    "vcs.ref_list",
    "vcs.ref_update",
    "vcs.git_import",
    "vcs.git_export",
];

/// Object-route authorization aliases (contract C2): classified in
/// `proc_classes` for `/api/vcs/*` route authorization — they are *not*
/// procedures and must never appear in `PROCEDURES` or gain dispatch arms.
pub const VCS_OBJECT_ROUTE_ALIASES: &[&str] = &["vcs.object_get", "vcs.object_put"];

// ---------------------------------------------------------------------------
// Activity-kind constants (contract C4 — the single source for
// `repo_activity.push_type` on mutating vcs.* ops, BR1.7).
// ---------------------------------------------------------------------------

/// `vcs.repo_enable` toggled the repo's VCS surface flag.
pub const VCS_ACTIVITY_REPO_ENABLE: &str = "vcs.repo_enable";
/// `vcs.changeset_create` minted a changeset.
pub const VCS_ACTIVITY_CHANGESET_CREATE: &str = "vcs.changeset_create";
/// `vcs.merge` produced a merge (or fast-path descendant).
pub const VCS_ACTIVITY_MERGE: &str = "vcs.merge";
/// `vcs.rebase` replayed a changeset onto a new parent.
pub const VCS_ACTIVITY_REBASE: &str = "vcs.rebase";
/// `vcs.ref_update` created/moved/deleted a ref.
pub const VCS_ACTIVITY_REF_UPDATE: &str = "vcs.ref_update";
/// `vcs.git_import` imported commits.
pub const VCS_ACTIVITY_GIT_IMPORT: &str = "vcs.git_import";
/// `vcs.git_export` wrote changesets out to git.
pub const VCS_ACTIVITY_GIT_EXPORT: &str = "vcs.git_export";

/// All seven activity kinds — the allow-list extension point for
/// `VALID_PUSH_TYPES` (u7's host wiring).
pub const VCS_ACTIVITY_KINDS: &[&str] = &[
    VCS_ACTIVITY_REPO_ENABLE,
    VCS_ACTIVITY_CHANGESET_CREATE,
    VCS_ACTIVITY_MERGE,
    VCS_ACTIVITY_REBASE,
    VCS_ACTIVITY_REF_UPDATE,
    VCS_ACTIVITY_GIT_IMPORT,
    VCS_ACTIVITY_GIT_EXPORT,
];

// ---------------------------------------------------------------------------
// Wire helpers
// ---------------------------------------------------------------------------

/// Required-nullable wire field: distinguishes an absent key (decode error —
/// the field is not `Option`, so serde raises `missing_field`), an explicit
/// `null` ([`Nullable::Null`]), and a present value ([`Nullable::Value`]).
///
/// serde's implicit `None`-default on `Option<T>` collapses absent and `null`
/// into one state, so only this encoding gives `vcs.ref_update` its CAS
/// semantics: `expected_old: null` = create-only, `new: null` = delete,
/// absent = `rpc.bad_input` (BR1.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Nullable<T> {
    /// Explicit `null` on the wire.
    Null,
    /// A present value.
    Value(T),
}

impl<T> Nullable<T> {
    /// True when the wire carried explicit `null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Borrowed view of the carried value (`None` on `Null`).
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Null => None,
            Self::Value(v) => Some(v),
        }
    }

    /// Consume into a plain `Option` (`Null` → `None`).
    pub fn into_option(self) -> Option<T> {
        match self {
            Self::Null => None,
            Self::Value(v) => Some(v),
        }
    }
}

/// Custom Deserialize: `null` → `Null`, `v` → `Value(v)`, absent →
/// `missing_field` (the field is not `Option`, so serde invokes this impl on a
/// `MissingFieldDeserializer` when the key is absent — and `deserialize_any`
/// errors there, while `deserialize_option` would have silently `visit_none`d
/// absent into `Null`). Buffering through `serde_json::Value` keeps the three
/// wire states distinct.
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Nullable<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::Null => Ok(Self::Null),
            v => T::deserialize(v)
                .map(Self::Value)
                .map_err(|e| serde::de::Error::custom(e.to_string())),
        }
    }
}

/// Untagged-style Serialize: `Null` emits bare `null`, `Value(v)` emits `v`.
/// A derived impl would tag the variants (`{"Null":…}`) — never on the wire.
impl<T: Serialize> Serialize for Nullable<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_none(),
            Self::Value(v) => v.serialize(serializer),
        }
    }
}

/// True when `s` is `cs_` + a 26-char Crockford-base32 ULID body (canonical
/// uppercase; the alphabet excludes I, L, O, U).
fn is_changeset_id(s: &str) -> bool {
    let Some(body) = s.strip_prefix("cs_") else {
        return false;
    };
    body.len() == 26
        && body.bytes().all(|b| {
            matches!(
                b,
                b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z'
            )
        })
}

/// True when `s` is `sha256:` + 64 lowercase hex chars.
fn is_object_id(s: &str) -> bool {
    let Some(hex) = s.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Opaque server-minted changeset identifier — `cs_<ulid>` on the wire,
/// never a git oid. Validated on construction/deserialize; compared by
/// equality only.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChangesetId(String);

impl ChangesetId {
    /// Validate the `cs_<ulid>` shape.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if is_changeset_id(raw) {
            Ok(Self(raw.to_string()))
        } else {
            Err(format!("invalid changeset id: {raw}"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ChangesetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ChangesetId> for String {
    fn from(id: ChangesetId) -> Self {
        id.0
    }
}

/// Transparent string on the wire.
impl Serialize for ChangesetId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ChangesetId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for ChangesetId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ChangesetId".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        String::json_schema(generator)
    }
}

/// Content-addressed object identifier — `sha256:`-prefixed lowercase hex on
/// the wire (the `/api/vcs/*` object routes' hash form). Validated on
/// construction/deserialize.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectId(String);

impl ObjectId {
    /// Validate the `sha256:<64 lowercase hex>` shape.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if is_object_id(raw) {
            Ok(Self(raw.to_string()))
        } else {
            Err(format!("invalid object id: {raw}"))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ObjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<ObjectId> for String {
    fn from(id: ObjectId) -> Self {
        id.0
    }
}

/// Transparent string on the wire.
impl Serialize for ObjectId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ObjectId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for ObjectId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ObjectId".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        String::json_schema(generator)
    }
}

// ---------------------------------------------------------------------------
// Error taxonomy (AC8.1.4 — closed set of wire-stable dotted codes).
// ---------------------------------------------------------------------------

/// Closed enum of `vcs.*` error codes emitted by the domain layer and the
/// `/api/vcs/*` routes. Per-variant serde rename: container `snake_case`
/// cannot emit the dotted code (`vcs_disabled` ≠ `vcs.disabled`). Transport
/// denials keep the platform `auth.*`/`rpc.*` taxonomy — the layers never
/// cross-contaminate (SR2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum VcsErrorCode {
    /// Repo VCS surface never enabled (or globally off) — denies everything.
    #[serde(rename = "vcs.disabled")]
    Disabled,
    /// Repo/ref/changeset/object not found (FG-uncovered targets use the same
    /// shape — anti-enumeration).
    #[serde(rename = "vcs.not_found")]
    NotFound,
    /// Domain-level input rejection (handler-side validation, not decode).
    #[serde(rename = "vcs.invalid_input")]
    InvalidInput,
    /// CAS expectation failed — `data.actual` carries the stored ref value.
    #[serde(rename = "vcs.ref_conflict")]
    RefConflict,
    /// Merge/rebase denial — `data.conflicts` lists the conflicting paths.
    #[serde(rename = "vcs.conflict")]
    Conflict,
    /// No merge base exists (merge_base/merge/rebase).
    #[serde(rename = "vcs.no_common_ancestor")]
    NoCommonAncestor,
    /// Object-size or repo-quota denial.
    #[serde(rename = "vcs.quota_exceeded")]
    QuotaExceeded,
    /// RPC-side caps (changeset >10_000 changes, inline >256 KiB/entry).
    #[serde(rename = "vcs.too_large")]
    TooLarge,
    /// `git_import` on a malformed cursor or a concurrent in-progress import.
    #[serde(rename = "vcs.import_state")]
    ImportState,
    /// Missing/cookie-only credential on the `/api/vcs/*` object routes.
    #[serde(rename = "vcs.unauthenticated")]
    Unauthenticated,
    /// Resolved but insufficient credential (or a protection verdict denial —
    /// `data.verdicts` mirrors the git-push path).
    #[serde(rename = "vcs.forbidden")]
    Forbidden,
}

impl VcsErrorCode {
    /// The dotted wire code (`vcs.disabled`, …).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "vcs.disabled",
            Self::NotFound => "vcs.not_found",
            Self::InvalidInput => "vcs.invalid_input",
            Self::RefConflict => "vcs.ref_conflict",
            Self::Conflict => "vcs.conflict",
            Self::NoCommonAncestor => "vcs.no_common_ancestor",
            Self::QuotaExceeded => "vcs.quota_exceeded",
            Self::TooLarge => "vcs.too_large",
            Self::ImportState => "vcs.import_state",
            Self::Unauthenticated => "vcs.unauthenticated",
            Self::Forbidden => "vcs.forbidden",
        }
    }

    /// `AppError` carrying this code and message (no `data` payload).
    pub fn app_error(self, message: impl Into<String>) -> AppError {
        AppError::new(self, message)
    }

    /// `vcs.ref_conflict` with the `{actual}` payload — the stored ref value
    /// the CAS expectation failed against (R-19); `None` when the ref is
    /// absent server-side.
    pub fn ref_conflict(message: impl Into<String>, actual: Option<ChangesetId>) -> AppError {
        Self::RefConflict.app_error(message).with_data(
            serde_json::to_value(VcsRefConflictData { actual })
                .unwrap_or_else(|_| serde_json::json!({})),
        )
    }

    /// `vcs.conflict` with the bounded `{conflicts, truncated}` payload (SR3):
    /// the constructor truncates at [`VCS_CONFLICT_DATA_MAX_ENTRIES`] and sets
    /// `truncated` rather than letting a caller mint an unbounded error body.
    pub fn conflict(message: impl Into<String>, conflicts: Vec<VcsConflictEntry>) -> AppError {
        let truncated = conflicts.len() > VCS_CONFLICT_DATA_MAX_ENTRIES;
        let data = VcsConflictData {
            conflicts: conflicts
                .into_iter()
                .take(VCS_CONFLICT_DATA_MAX_ENTRIES)
                .collect(),
            truncated,
        };
        Self::Conflict
            .app_error(message)
            .with_data(serde_json::to_value(data).unwrap_or_else(|_| serde_json::json!({})))
    }

    /// `vcs.forbidden` with the `{verdicts}` payload — the same protection
    /// verdict block a git push denial produces (AC7.2.2).
    pub fn forbidden(message: impl Into<String>, verdicts: ProtectionBlockReasons) -> AppError {
        Self::Forbidden.app_error(message).with_data(
            serde_json::to_value(VcsForbiddenData { verdicts })
                .unwrap_or_else(|_| serde_json::json!({})),
        )
    }
}

impl From<VcsErrorCode> for String {
    fn from(code: VcsErrorCode) -> Self {
        code.as_str().to_string()
    }
}

/// Cap on `vcs.conflict` `data.conflicts` entries — the vcs page bound (SR3;
/// contract `limits.pagination` ≤500).
pub const VCS_CONFLICT_DATA_MAX_ENTRIES: usize = 500;

/// `vcs.ref_conflict` `data` payload — the stored ref value the CAS expected.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRefConflictData {
    /// Stored value; `null` when the ref does not exist server-side.
    pub actual: Option<ChangesetId>,
}

/// One conflicting path inside a `vcs.conflict` payload.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsConflictEntry {
    pub path: String,
    /// Open string — the merge engine's conflict taxonomy
    /// (binary/mode/type/rename); a new kind is additive on the wire.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// `vcs.conflict` `data` payload — bounded at
/// [`VCS_CONFLICT_DATA_MAX_ENTRIES`] by the `VcsErrorCode::conflict`
/// constructor.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsConflictData {
    pub conflicts: Vec<VcsConflictEntry>,
    /// True when the conflict set exceeded the bound and was truncated.
    pub truncated: bool,
}

/// `vcs.forbidden` `data` payload — protection verdicts on the git-export
/// denial path, nested under the `verdicts` key (AC7.2.2).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsForbiddenData {
    pub verdicts: ProtectionBlockReasons,
}

// ---------------------------------------------------------------------------
// Shared public shapes
// ---------------------------------------------------------------------------

/// Changeset author — `{name, email}`. Optional on `changeset_create`
/// (resolved server-side from the authenticated user when absent); always
/// present on read rows.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsAuthor {
    pub name: String,
    pub email: String,
}

/// File-change kind — `add` | `modify` | `delete` | `rename`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ManifestEntryKind {
    Add,
    Modify,
    Delete,
    Rename,
}

/// Octal file mode carried as a string — `"100644"` | `"100755"` | `"120000"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub enum VcsFileMode {
    /// Regular file.
    #[serde(rename = "100644")]
    Regular,
    /// Executable file.
    #[serde(rename = "100755")]
    Executable,
    /// Symlink.
    #[serde(rename = "120000")]
    Symlink,
}

/// One changed-file row — `changeset_create` inputs and
/// `changeset_get`/`log` manifests share this shape. Exactly one of `blob`
/// (existing object) or `content_b64` (inline ≤256 KiB) rides a non-delete
/// entry; `delete` carries neither; `rename` carries `old_path`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ManifestEntry {
    pub path: String,
    pub kind: ManifestEntryKind,
    pub mode: VcsFileMode,
    /// Source path when `kind` = `rename`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    /// Existing object reference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<ObjectId>,
    /// Inline content transport (≤256 KiB).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_b64: Option<String>,
}

/// One changeset row — the `vcs.changeset_get` body and `vcs.log` entries
/// share this shape.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsChangesetPublic {
    pub changeset_id: ChangesetId,
    pub parents: Vec<ChangesetId>,
    pub author: VcsAuthor,
    pub message: String,
    pub committed_at: String,
    pub manifest: Vec<ManifestEntry>,
}

/// One named head on `vcs.status` — `{name, changeset_id}`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsHeadPublic {
    pub name: String,
    pub changeset_id: ChangesetId,
}

/// `vcs.ref_list` entry — a listed ref always resolves to a changeset
/// (non-null per the C1 freeze; delete-result nullability lives only on
/// `VcsRefUpdateResponse`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRefPublic {
    pub name: String,
    pub changeset_id: ChangesetId,
    pub updated_at: String,
}

// ---------------------------------------------------------------------------
// Per-procedure DTO pairs (C1 — every request carries {owner, name} first).
// ---------------------------------------------------------------------------

/// `vcs.repo_enable` input — toggle the repo's VCS surface flag.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRepoEnableRequest {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRepoEnableResponse {
    pub enabled: bool,
}

/// `vcs.status` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsStatusRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsStatusResponse {
    pub enabled: bool,
    pub heads: Vec<VcsHeadPublic>,
    pub changeset_count: u64,
    pub object_count: u64,
    pub storage_bytes: u64,
}

/// `vcs.changeset_create` input — `author` resolves server-side from the
/// authenticated user when absent.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsChangesetCreateRequest {
    pub owner: String,
    pub name: String,
    /// Parent changeset ids (`[]` = root).
    pub parents: Vec<ChangesetId>,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<VcsAuthor>,
    pub changes: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsChangesetCreateResponse {
    pub changeset_id: ChangesetId,
    pub manifest_hash: String,
}

/// `vcs.changeset_get` input — `id` accepts a `changeset_id` or a `git_oid`
/// (a git-oid miss triggers lazy import then re-resolves, AC7.1.2).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsChangesetGetRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
}

/// `vcs.changeset_get` body — the shared changeset row shape.
pub type VcsChangesetGetResponse = VcsChangesetPublic;

/// `vcs.log` input — `from` accepts `ref_name | changeset_id | git_oid`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsLogRequest {
    pub owner: String,
    pub name: String,
    pub from: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    /// Opaque pagination cursor — absent in the response means end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsLogResponse {
    pub entries: Vec<VcsChangesetPublic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// `vcs.diff` patch — inline text, or a `{truncated: true}` marker when the
/// per-file patch exceeds the 256 KiB cap.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum VcsDiffPatch {
    Text(String),
    Truncated { truncated: bool },
}

/// One changed file in a `vcs.diff` page.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsDiffEntry {
    pub path: String,
    pub kind: ManifestEntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_mode: Option<VcsFileMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_mode: Option<VcsFileMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_blob: Option<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_blob: Option<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<VcsDiffPatch>,
}

/// Aggregate counters across the whole diff (unpaged).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsDiffTotals {
    pub files: u64,
    pub additions: u64,
    pub deletions: u64,
}

/// `vcs.diff` input — bounded at 500 files/call, paginated by file.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsDiffRequest {
    pub owner: String,
    pub name: String,
    pub from: ChangesetId,
    pub to: ChangesetId,
    /// Multi-value path filter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsDiffResponse {
    pub entries: Vec<VcsDiffEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub totals: VcsDiffTotals,
}

/// `vcs.tree` entry — one row of a changeset's file listing.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsTreeEntry {
    pub path: String,
    pub mode: VcsFileMode,
    pub blob: ObjectId,
}

/// `vcs.tree` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsTreeRequest {
    pub owner: String,
    pub name: String,
    pub changeset_id: ChangesetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsTreeResponse {
    pub entries: Vec<VcsTreeEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// `vcs.file_get` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsFileGetRequest {
    pub owner: String,
    pub name: String,
    pub changeset_id: ChangesetId,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsFileGetResponse {
    pub blob: ObjectId,
    pub mode: VcsFileMode,
    pub size: u64,
    /// Inline content — `null` is meaningful (over the 256 KiB inline cap →
    /// `too_large`, fetch via the object route). Always emitted.
    pub content_b64: Option<String>,
    pub too_large: bool,
}

/// `vcs.blame` entry — one line's last-touch attribution.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsBlameEntry {
    pub changeset_id: ChangesetId,
    pub author: VcsAuthor,
    pub content: String,
}

/// `vcs.blame` input — `at` omitted resolves `main`'s tip (or the sole head
/// when exactly one ref exists).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsBlameRequest {
    pub owner: String,
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<ChangesetId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsBlameResponse {
    pub entries: Vec<VcsBlameEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// `vcs.path_history` entry — one change touching `path`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsPathHistoryEntry {
    pub changeset_id: ChangesetId,
    pub kind: ManifestEntryKind,
    pub path: String,
    pub committed_at: String,
}

/// `vcs.path_history` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsPathHistoryRequest {
    pub owner: String,
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsPathHistoryResponse {
    pub entries: Vec<VcsPathHistoryEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// `vcs.merge_base` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsMergeBaseRequest {
    pub owner: String,
    pub name: String,
    pub a: ChangesetId,
    pub b: ChangesetId,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsMergeBaseResponse {
    pub merge_base: ChangesetId,
}

/// `vcs.merge` input — `a`/`b` are the two heads to merge (wire aligns with
/// `vcs.merge_base`; the internal merge(ours,theirs) naming stays internal).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsMergeRequest {
    pub owner: String,
    pub name: String,
    pub a: ChangesetId,
    pub b: ChangesetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsMergeResponse {
    /// The merge result — an ancestor/descendant pair yields the descendant
    /// (no merge node).
    pub changeset_id: ChangesetId,
    /// Present when the fast-path applied (no merge node written).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fast_forward: Option<bool>,
}

/// `vcs.rebase` input — replays ONE changeset's changed files onto
/// `new_parent_id` (chain replay is a client loop, not a wire range).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRebaseRequest {
    pub owner: String,
    pub name: String,
    pub changeset_id: ChangesetId,
    pub new_parent_id: ChangesetId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRebaseResponse {
    pub head: ChangesetId,
}

/// `vcs.ref_list` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRefListRequest {
    pub owner: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRefListResponse {
    pub entries: Vec<VcsRefPublic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// `vcs.ref_update` input — CAS on a named ref. `expected_old`/`new` are
/// required-nullable [`Nullable`]: absent = `rpc.bad_input`,
/// `expected_old: null` = create-only, `new: null` = delete. Non-idempotent —
/// a retried landed call fails `vcs.ref_conflict` (AC5.1.3).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRefUpdateRequest {
    pub owner: String,
    pub name: String,
    #[serde(rename = "ref")]
    pub ref_name: String,
    #[schemars(with = "Option<ChangesetId>")]
    pub expected_old: Nullable<ChangesetId>,
    #[schemars(with = "Option<ChangesetId>")]
    pub new: Nullable<ChangesetId>,
}

/// `vcs.ref_update` result — `changeset_id` is `null` after a delete
/// (required field, always emitted).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsRefUpdateResponse {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub changeset_id: Option<ChangesetId>,
}

/// `vcs.git_import` head — where an imported branch landed.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsGitImportHead {
    pub name: String,
    pub git_oid: String,
    pub changeset_id: ChangesetId,
}

/// `vcs.git_import` input — `from` accepts `ref | git_oid`; resumable via
/// `cursor` (≤1000 commits/call).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsGitImportRequest {
    pub owner: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsGitImportResponse {
    pub imported: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub done: bool,
    pub heads: Vec<VcsGitImportHead>,
}

/// `vcs.git_export` input — `force` overrides fast-forward only.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsGitExportRequest {
    pub owner: String,
    pub name: String,
    pub head: ChangesetId,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsGitExportResponse {
    pub branch: String,
    pub git_oid: String,
}

/// `/api/vcs/{owner}/{name}/objects` POST result (contract C2) — the
/// server-computed content hash.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VcsObjectPutResponse {
    pub hash: ObjectId,
}

#[cfg(test)]
mod tests {
    use super::*;

    const CS_A: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR";
    const CS_B: &str = "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQS";
    const OBJ: &str = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn cs(raw: &str) -> ChangesetId {
        ChangesetId::parse(raw).unwrap()
    }

    #[test]
    fn changeset_id_validates_cs_ulid_shape() {
        assert!(ChangesetId::parse(CS_A).is_ok());
        assert!(ChangesetId::parse("cs_00000000000000000000000000").is_ok());
        for bad in [
            "",
            "cs_",
            "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQ",   // 25 chars
            "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQRX", // 27 chars
            "01H8V4JGZ0K2B9Q7X3F5M6NPQR",     // no prefix
            "cs_01H8V4JGZ0K2B9Q7X3F5M6NPIR",  // I is not base32
            "cs_01H8V4JGZ0K2B9Q7X3F5M6NPLR",  // L is not base32
            "cs_01H8V4JGZ0K2B9Q7X3F5M6NPOR",  // O is not base32
            "cs_01H8V4JGZ0K2B9Q7X3F5M6NPUR",  // U is not base32
            "cs_01h8v4jgz0k2b9q7x3f5m6npqr",  // lowercase
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(ChangesetId::parse(bad).is_err(), "{bad} must be rejected");
        }
        let id = cs(CS_A);
        assert_eq!(id.as_str(), CS_A);
        assert_eq!(id.to_string(), CS_A);
        assert_eq!(String::from(id.clone()), CS_A);
    }

    #[test]
    fn object_id_validates_sha256_hex_shape() {
        assert!(ObjectId::parse(OBJ).is_ok());
        for bad in [
            "",
            "sha256:",
            "sha256:01234",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "sha256:0123456789ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef",
            "sha1:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdeg",
        ] {
            assert!(ObjectId::parse(bad).is_err(), "{bad} must be rejected");
        }
        let id = ObjectId::parse(OBJ).unwrap();
        assert_eq!(id.as_str(), OBJ);
        assert_eq!(id.to_string(), OBJ);
        assert_eq!(String::from(id.clone()), OBJ);
    }

    #[test]
    fn id_newtypes_serialize_transparent() {
        assert_eq!(serde_json::to_value(cs(CS_A)).unwrap(), CS_A);
        let obj = ObjectId::parse(OBJ).unwrap();
        assert_eq!(serde_json::to_value(&obj).unwrap(), OBJ);
    }

    #[test]
    fn changeset_id_field_rejects_bad_wire_value() {
        let err = serde_json::from_value::<VcsMergeBaseRequest>(serde_json::json!({
            "owner": "o",
            "name": "n",
            "a": "not-a-cs-id",
            "b": CS_B,
        }))
        .unwrap_err();
        assert!(err.to_string().contains("invalid changeset id"), "{err}");
    }

    #[test]
    fn nullable_trichotomy() {
        // value → Value
        let req: VcsRefUpdateRequest = serde_json::from_value(serde_json::json!({
            "owner": "o",
            "name": "n",
            "ref": "main",
            "expected_old": CS_A,
            "new": CS_B,
        }))
        .unwrap();
        assert_eq!(req.expected_old, Nullable::Value(cs(CS_A)));
        assert_eq!(req.new, Nullable::Value(cs(CS_B)));
        assert!(!req.expected_old.is_null());
        assert_eq!(req.new.value(), Some(&cs(CS_B)));

        // explicit null → Null (create-only / delete)
        let req: VcsRefUpdateRequest = serde_json::from_value(serde_json::json!({
            "owner": "o",
            "name": "n",
            "ref": "main",
            "expected_old": null,
            "new": null,
        }))
        .unwrap();
        assert_eq!(req.expected_old, Nullable::Null);
        assert!(req.expected_old.is_null());
        assert_eq!(req.expected_old.value(), None);
        assert_eq!(req.new.into_option(), None);

        // absent → missing_field decode error (Option would silently default)
        let err = serde_json::from_value::<VcsRefUpdateRequest>(serde_json::json!({
            "owner": "o",
            "name": "n",
            "ref": "main",
            "expected_old": null,
        }))
        .unwrap_err();
        assert!(err.to_string().contains("new"), "{err}");
    }

    #[test]
    fn nullable_serializes_bare_null_or_value() {
        let req = VcsRefUpdateRequest {
            owner: "o".into(),
            name: "n".into(),
            ref_name: "main".into(),
            expected_old: Nullable::Null,
            new: Nullable::Value(cs(CS_A)),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["expected_old"], serde_json::Value::Null);
        assert_eq!(v["new"], serde_json::json!(CS_A));
    }

    #[test]
    fn ref_field_renames_on_the_wire() {
        let req = VcsRefUpdateRequest {
            owner: "o".into(),
            name: "n".into(),
            ref_name: "main".into(),
            expected_old: Nullable::Null,
            new: Nullable::Null,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["ref"], "main");
        assert!(v.get("ref_name").is_none(), "field must serialize as `ref`");
        let back: VcsRefUpdateRequest = serde_json::from_value(v).unwrap();
        assert_eq!(back.ref_name, "main");

        let resp = VcsRefUpdateResponse {
            ref_name: "main".into(),
            changeset_id: None,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(v["ref"], "main");
        // A deleted ref resolves to null — the null is emitted, not omitted.
        assert_eq!(v["changeset_id"], serde_json::Value::Null);
    }

    #[test]
    fn error_codes_serialize_dotted() {
        let cases: [(VcsErrorCode, &str); 11] = [
            (VcsErrorCode::Disabled, "vcs.disabled"),
            (VcsErrorCode::NotFound, "vcs.not_found"),
            (VcsErrorCode::InvalidInput, "vcs.invalid_input"),
            (VcsErrorCode::RefConflict, "vcs.ref_conflict"),
            (VcsErrorCode::Conflict, "vcs.conflict"),
            (VcsErrorCode::NoCommonAncestor, "vcs.no_common_ancestor"),
            (VcsErrorCode::QuotaExceeded, "vcs.quota_exceeded"),
            (VcsErrorCode::TooLarge, "vcs.too_large"),
            (VcsErrorCode::ImportState, "vcs.import_state"),
            (VcsErrorCode::Unauthenticated, "vcs.unauthenticated"),
            (VcsErrorCode::Forbidden, "vcs.forbidden"),
        ];
        for (code, dotted) in cases {
            assert_eq!(code.as_str(), dotted);
            assert_eq!(serde_json::to_value(code).unwrap(), dotted);
            let back: VcsErrorCode = serde_json::from_value(serde_json::json!(dotted)).unwrap();
            assert_eq!(back, code);
            assert_eq!(String::from(code), dotted);
        }
        assert!(serde_json::from_value::<VcsErrorCode>(serde_json::json!("vcs_disabled")).is_err());
        assert!(serde_json::from_value::<VcsErrorCode>(serde_json::json!("vcs.bogus")).is_err());
    }

    #[test]
    fn error_data_shapes() {
        // ref_conflict — {actual} carries the stored value (or null).
        let err = VcsErrorCode::ref_conflict("cas failed", Some(cs(CS_A)));
        assert_eq!(err.code, "vcs.ref_conflict");
        assert_eq!(err.data.unwrap()["actual"], serde_json::json!(CS_A));
        let err = VcsErrorCode::ref_conflict("cas failed", None);
        assert_eq!(err.data.unwrap()["actual"], serde_json::Value::Null);

        // conflict — bounded at 500, truncated flag set by the constructor.
        let small = VcsErrorCode::conflict(
            "merge conflicts",
            vec![VcsConflictEntry {
                path: "a.txt".into(),
                kind: "binary".into(),
                detail: None,
            }],
        );
        let data = small.data.unwrap();
        assert_eq!(data["truncated"], false);
        assert_eq!(data["conflicts"][0]["path"], "a.txt");
        let big = VcsErrorCode::conflict(
            "merge conflicts",
            (0..600)
                .map(|i| VcsConflictEntry {
                    path: format!("f{i}"),
                    kind: "text".into(),
                    detail: Some("both modified".into()),
                })
                .collect(),
        );
        let data = big.data.unwrap();
        assert_eq!(data["truncated"], true);
        assert_eq!(data["conflicts"].as_array().unwrap().len(), 500);

        // forbidden — {verdicts} nests the protection payload.
        let err = VcsErrorCode::forbidden(
            "protected branch",
            ProtectionBlockReasons {
                reasons: vec!["protected".into()],
                ..Default::default()
            },
        );
        assert_eq!(err.code, "vcs.forbidden");
        assert_eq!(
            err.data.unwrap()["verdicts"]["reasons"][0],
            serde_json::json!("protected")
        );
    }

    #[test]
    fn optionals_serialize_absent_not_null() {
        // Request-side optionals are omitted when unset.
        let req = VcsLogRequest {
            owner: "o".into(),
            name: "n".into(),
            from: "main".into(),
            limit: None,
            cursor: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert!(v.get("limit").is_none());
        assert!(v.get("cursor").is_none());
        // …and absent decodes to None.
        let back: VcsLogRequest = serde_json::from_value(serde_json::json!({
            "owner": "o",
            "name": "n",
            "from": "main",
        }))
        .unwrap();
        assert_eq!(back.limit, None);

        // Response-side cursor absent = end (BR1.5).
        let resp = VcsLogResponse {
            entries: vec![],
            cursor: None,
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert!(v.get("cursor").is_none());

        // ManifestEntry optionals omitted; content_b64 on file_get emits null.
        let entry = ManifestEntry {
            path: "a".into(),
            kind: ManifestEntryKind::Add,
            mode: VcsFileMode::Regular,
            old_path: None,
            blob: None,
            content_b64: None,
        };
        let v = serde_json::to_value(&entry).unwrap();
        for key in ["old_path", "blob", "content_b64"] {
            assert!(v.get(key).is_none(), "{key} must be omitted when unset");
        }
        assert_eq!(v["kind"], "add");
        assert_eq!(v["mode"], "100644");

        let got = VcsFileGetResponse {
            blob: ObjectId::parse(OBJ).unwrap(),
            mode: VcsFileMode::Symlink,
            size: 12,
            content_b64: None,
            too_large: true,
        };
        let v = serde_json::to_value(&got).unwrap();
        assert_eq!(
            v["content_b64"],
            serde_json::Value::Null,
            "content_b64: null is meaningful (over the inline cap)"
        );
        assert_eq!(v["mode"], "120000");
        assert_eq!(v["blob"], OBJ);
    }

    #[test]
    fn request_owner_name_required() {
        for missing in [
            serde_json::json!({"name": "n", "enabled": true}),
            serde_json::json!({"owner": "o", "enabled": true}),
        ] {
            assert!(
                serde_json::from_value::<VcsRepoEnableRequest>(missing.clone()).is_err(),
                "{missing} must fail decode — owner/name are separate required fields"
            );
        }
    }

    #[test]
    fn changeset_round_trip() {
        let row = VcsChangesetPublic {
            changeset_id: cs(CS_A),
            parents: vec![cs(CS_B)],
            author: VcsAuthor {
                name: "A U Thor".into(),
                email: "a@example.com".into(),
            },
            message: "msg".into(),
            committed_at: "2026-10-18T00:00:00Z".into(),
            manifest: vec![ManifestEntry {
                path: "src/main.rs".into(),
                kind: ManifestEntryKind::Rename,
                mode: VcsFileMode::Executable,
                old_path: Some("src/lib.rs".into()),
                blob: Some(ObjectId::parse(OBJ).unwrap()),
                content_b64: None,
            }],
        };
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["changeset_id"], CS_A);
        assert_eq!(v["manifest"][0]["kind"], "rename");
        assert_eq!(v["manifest"][0]["mode"], "100755");
        assert_eq!(v["manifest"][0]["old_path"], "src/lib.rs");
        let back: VcsChangesetPublic = serde_json::from_value(v).unwrap();
        assert_eq!(back.changeset_id, cs(CS_A));
        assert_eq!(back.manifest[0].kind, ManifestEntryKind::Rename);
        assert_eq!(back.manifest[0].mode, VcsFileMode::Executable);
    }
}
