//! Git ↔ changeset interop (u6) — the bridge between the forge's git object
//! graph and the VCS changeset DAG.
//!
//! **Import** (`import_chain`, `lazy_import`): resolve tips (explicit `from`
//! or heads+tags), walk `rev_list` parents-first inside a nonce-claimed
//! `active` marker, mint changesets through `ChangesetStore::create`, and
//! converge identity through `vcs_import_links` (`git_oid ↔ changeset_id`)
//! plus the `linked:` reverse row — re-runs are idempotent and the opaque
//! `CursorState` snapshot resumes paging across calls.
//!
//! **Export** (`export`): post-order DFS the changeset DAG to the first
//! linked+verifiable anchor, mint every missing git blob/tree/commit
//! deterministically (idents restored byte-exact from `ident-cs:` sidecars),
//! run the host `ProtectionChecker`, then perform exactly ONE hook-gated ref
//! move. No ref writes happen before the check; a denial leaves only
//! unreferenced objects that gc reaps.
//!
//! No new tables — five keyspaces ride `vcs_derived_index` under
//! `kind="vcs:import"`: `active` (claim), `cursor` (resume snapshot),
//! `ident:{oid}` / `ident-cs:{cs}` (raw ident sidecars), `linked:{cs}`
//! (changeset→git_oid reverse map). `covering_changeset` is `NULL` on the
//! marker rows — migration 0055 made the column nullable.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use base64::Engine;
use futures_util::stream;
use oxidean_core::protection_types::ProtectionBlockReasons;
use oxidean_core::vcs_types::{ChangesetId, VcsErrorCode};
use oxidean_core::AppError;
use oxidean_db::{Database, VcsDerivedIndexUpsert};
use oxidean_git::{
    FlatTreeEntry, GitBackend, GitError, GitIdent, NameStatusKind, TreeEntryKind, TreeWriteEntry,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;
use tracing::Instrument;

use crate::changeset_store::{
    ChangeKind, ChangesetError, ChangesetStore, CreateChangesetInput, FileChangeInput,
};
use crate::object_store::{ObjectStore, ObjectStoreError, QuotaScope};
use crate::read_index::{IndexError, ReadIndex};

/// Derived-index kind namespace for every u6 marker/lookup row.
const KIND_IMPORT: &str = "vcs:import";
const KEY_ACTIVE: &str = "active";
const KEY_CURSOR: &str = "cursor";

/// Mirrored store bounds — `create` re-validates authoritatively; the import
/// refusal inventory trips them BEFORE any object/link write (BR6.23).
const MAX_PARENTS: usize = 64;
const MAX_MESSAGE_BYTES: usize = 65_535;
const MAX_AUTHOR_CHARS: usize = 255;
const MAX_PATH_CHARS: usize = 512;
/// Legal leaf modes — regular/executable/symlink; tree (16384) is a manifest
/// dir entry, gitlink (`160000`) is refused on import (BR6.23).
const LEGAL_LEAF_MODES: [i64; 3] = [0o100644, 0o100755, 0o120000];
const MODE_GITLINK: &str = "160000";

/// `tree_flat` / `commit_name_status` stdout bound multipliers (TD6.2 —
/// caller `import_tree_max` × avg-record bytes).
const FLAT_RECORD_AVG: usize = 1024;
const STATUS_RECORD_AVG: usize = 2048;

// ---------------------------------------------------------------------------
// Error surface + bridges
// ---------------------------------------------------------------------------

/// Typed failure surface — u7 maps each variant to a wire code via
/// [`to_app_error`]. Deliberately no `PartialEq` (u5 convention); messages
/// carry `field: reason` text and NEVER echo rejected values, git stderr,
/// or paths (SD6.4).
#[derive(Debug)]
pub enum InteropError {
    /// `Database::is_skipped()` — eager fail-closed entry gate.
    DbNotConfigured,
    /// A ref/rev/changeset resolved to nothing.
    NotFound,
    /// Fail-closed input rejection — `field: reason` text only.
    InvalidInput(String),
    /// Cursor malformed/drifted or a concurrent import holds the claim.
    ImportState(String),
    /// Protection verdicts denied the export move — wire-verbatim payload.
    Forbidden { verdicts: ProtectionBlockReasons },
    /// CAS/ff expectation failed — `actual` is the linked changeset of the
    /// current git tip (`None` when the tip is git-only/unlinked).
    RefConflict {
        refname: String,
        actual: Option<ChangesetId>,
    },
    /// A named cap tripped — honest bound, never truncation.
    TooLarge,
    /// `ObjectStore::put` quota denial.
    QuotaExceeded(QuotaScope),
    /// Raw detail for host diagnostics — opaque to the wire.
    Internal(String),
}

impl std::fmt::Display for InteropError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DbNotConfigured => write!(f, "database not configured"),
            Self::NotFound => write!(f, "not found"),
            Self::InvalidInput(reason) => write!(f, "invalid input: {reason}"),
            Self::ImportState(reason) => write!(f, "import state: {reason}"),
            Self::Forbidden { .. } => write!(f, "forbidden by protection policy"),
            Self::RefConflict { .. } => write!(f, "ref conflict"),
            Self::TooLarge => write!(f, "too large"),
            Self::QuotaExceeded(scope) => {
                write!(f, "{}", ObjectStoreError::QuotaExceeded(*scope))
            }
            Self::Internal(detail) => write!(f, "internal error: {detail}"),
        }
    }
}

impl std::error::Error for InteropError {}

/// Every typed-error return warns once at construction — `%e` carries only
/// the typed error (SD6.4: no paths, rejected values, or stderr).
fn warned(e: InteropError) -> InteropError {
    tracing::warn!(error = %e, "vcs git-interop typed error");
    e
}

fn internal(detail: String) -> InteropError {
    warned(InteropError::Internal(detail))
}

fn invalid(field: &str, reason: &str) -> InteropError {
    warned(InteropError::InvalidInput(format!("{field}: {reason}")))
}

fn import_state(reason: &str) -> InteropError {
    warned(InteropError::ImportState(reason.to_string()))
}

fn too_large() -> InteropError {
    warned(InteropError::TooLarge)
}

fn not_found() -> InteropError {
    warned(InteropError::NotFound)
}

/// The fixed stdout-bound payload emitted by `oxidean-git`'s bounded reader
/// (`cli.rs STDOUT_BOUND_SENTINEL`) — the ONLY `Process` text that maps to
/// `TooLarge` (SD6.6). Duplicated deliberately: the sentinel contract is
/// content-matched across the crate seam.
const STDOUT_BOUND_SENTINEL: &str = "oxidean:stdout-bound-exceeded";

/// The complete git-error bridge (Q7): hook/`update` deny → Forbidden,
/// non-FF → RefConflict, bound-kill → TooLarge, everything else Internal.
fn map_git_err(e: GitError) -> InteropError {
    match e {
        GitError::Denied(_) => warned(InteropError::Forbidden {
            verdicts: hook_denied_verdicts(),
        }),
        // Export resolves `refname`/`actual` itself — this arm only serves
        // non-export callers, so both fields stay empty there.
        GitError::Conflict(_) => warned(InteropError::RefConflict {
            refname: String::new(),
            actual: None,
        }),
        GitError::NotFound(_) => not_found(),
        GitError::InvalidArg(msg) => {
            // Backend rejection reasons are static text — value-free by
            // construction at the interop boundary.
            warned(InteropError::InvalidInput(msg))
        }
        GitError::Process(msg) if msg == STDOUT_BOUND_SENTINEL => too_large(),
        other => internal(other.to_string()),
    }
}

/// The IndexError bridge — interop never feeds caller cursors to
/// `index.tree`, so `CursorMalformed` is corruption, never input (SD6.7).
/// Dead until a host read path consumes `index`; pinned for SD6.7.
#[allow(dead_code)]
fn map_index_err(e: IndexError) -> InteropError {
    match e {
        IndexError::CursorMalformed => internal(e.to_string()),
        IndexError::FrontierTooLarge => too_large(),
        IndexError::DbNotConfigured => warned(InteropError::DbNotConfigured),
        other => internal(other.to_string()),
    }
}

fn map_object_err(e: ObjectStoreError) -> InteropError {
    match e {
        ObjectStoreError::QuotaExceeded(scope) => warned(InteropError::QuotaExceeded(scope)),
        ObjectStoreError::DbNotConfigured => warned(InteropError::DbNotConfigured),
        other => internal(other.to_string()),
    }
}

fn map_changeset_err(e: ChangesetError) -> InteropError {
    match e {
        ChangesetError::ParentNotFound | ChangesetError::ChangesetNotFound => not_found(),
        ChangesetError::InvalidInput(reason) => warned(InteropError::InvalidInput(reason)),
        ChangesetError::DbNotConfigured => warned(InteropError::DbNotConfigured),
        other => internal(other.to_string()),
    }
}

/// Facade `Result<_, String>` — raw detail rides inside `Internal` (opaque
/// to the wire, visible to host diagnostics).
fn map_db_err(e: String) -> InteropError {
    internal(e)
}

/// The fixed verdict block a hook-denied move surfaces — `push_hook` reason,
/// no stderr echo (SD6.4/AC7.2.2 parity).
fn hook_denied_verdicts() -> ProtectionBlockReasons {
    ProtectionBlockReasons {
        reasons: vec!["push_hook".to_string()],
        ..Default::default()
    }
}

/// Wire bridge for u7 — every variant lands on an existing code; typed
/// payloads ride `data` (`verdicts`/`actual`), `Internal` is opaque.
pub fn to_app_error(e: &InteropError) -> AppError {
    match e {
        InteropError::DbNotConfigured => {
            AppError::new("db.not_configured", "vcs database not configured")
        }
        InteropError::NotFound => VcsErrorCode::NotFound.app_error("git interop: not found"),
        InteropError::InvalidInput(reason) => {
            VcsErrorCode::InvalidInput.app_error(format!("git interop: {reason}"))
        }
        InteropError::ImportState(reason) => {
            VcsErrorCode::ImportState.app_error(format!("git interop: {reason}"))
        }
        InteropError::Forbidden { verdicts } => {
            VcsErrorCode::forbidden("git export denied by protection policy", verdicts.clone())
        }
        InteropError::RefConflict { refname, actual } => VcsErrorCode::ref_conflict(
            format!("git export ref conflict: {refname}"),
            actual.clone(),
        ),
        InteropError::TooLarge => VcsErrorCode::TooLarge.app_error("git interop: bound exceeded"),
        InteropError::QuotaExceeded(_) => {
            VcsErrorCode::QuotaExceeded.app_error("git interop: quota exceeded")
        }
        InteropError::Internal(_) => AppError::new("internal", "internal error"),
    }
}

// ---------------------------------------------------------------------------
// Caps
// ---------------------------------------------------------------------------

/// Host-tunable bounds — constructor-injected so tests trip them with small
/// fixtures (u5 `MergeCaps` convention). `validate` runs at `new`.
#[derive(Debug, Clone)]
pub struct InteropCaps {
    /// `git_import.limit` ceiling (commits per call).
    pub import_commit_cap: usize,
    /// `rev_list` walk bound — total oids a single page may fetch.
    pub rev_list_max: usize,
    /// Snapshot `tips` ceiling inside a cursor.
    pub cursor_tips_max: usize,
    /// Serialized `CursorState` byte ceiling — the mysql `payload TEXT`
    /// column holds 64KiB; over ⇒ `TooLarge`, never an unresumable cursor.
    pub cursor_payload_max_bytes: usize,
    /// Tree-leaf accumulation bound (import flat walk + export fold).
    pub import_tree_max: usize,
    /// Imported blob byte cap — pinned ≤ `objects.max_object_bytes`.
    pub import_blob_max: u64,
    /// Export chain-walk bound (post-order DFS visited set).
    pub export_chain_max: usize,
    /// Exported blob byte cap — pinned ≤ `objects.max_object_bytes`.
    pub export_blob_max: u64,
    /// ff-gate changeset-ancestry BFS bound.
    pub export_ancestor_probe_max: usize,
    /// Import claim marker TTL — a stale `active` row may be reclaimed.
    pub import_lock_ttl_secs: i64,
}

impl InteropCaps {
    /// Defaults; `objects_max` feeds the two blob caps (u5 convention).
    pub fn new(objects_max: u64) -> Self {
        Self {
            import_commit_cap: 1_000,
            rev_list_max: 250_000,
            cursor_tips_max: 512,
            cursor_payload_max_bytes: 32_768,
            import_tree_max: 10_000,
            import_blob_max: objects_max,
            export_chain_max: 10_000,
            export_blob_max: objects_max,
            export_ancestor_probe_max: 250_000,
            import_lock_ttl_secs: 900,
        }
    }

    /// Fail-closed host-config check: zero caps rejected, blob caps must not
    /// exceed the object-store ceiling, and the stdout-bound multipliers
    /// must not wrap (`usize::MAX / 2048` headroom covers both).
    pub fn validate(&self, objects_max: u64) -> Result<(), InteropError> {
        for (name, v) in [
            ("import_commit_cap", self.import_commit_cap),
            ("rev_list_max", self.rev_list_max),
            ("cursor_tips_max", self.cursor_tips_max),
            ("cursor_payload_max_bytes", self.cursor_payload_max_bytes),
            ("import_tree_max", self.import_tree_max),
            ("export_chain_max", self.export_chain_max),
            ("export_ancestor_probe_max", self.export_ancestor_probe_max),
        ] {
            if v == 0 {
                return Err(invalid(name, "must be non-zero"));
            }
            if v > usize::MAX / STATUS_RECORD_AVG {
                return Err(invalid(name, "exceeds the multiplier headroom"));
            }
        }
        if self.import_blob_max == 0 || self.import_blob_max > objects_max {
            return Err(invalid(
                "import_blob_max",
                "must be 1..=object-store max_object_bytes",
            ));
        }
        if self.export_blob_max == 0 || self.export_blob_max > objects_max {
            return Err(invalid(
                "export_blob_max",
                "must be 1..=object-store max_object_bytes",
            ));
        }
        if self.import_lock_ttl_secs <= 0 {
            return Err(invalid("import_lock_ttl_secs", "must be positive"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wire-facing result types
// ---------------------------------------------------------------------------

/// One page of `git_import` — `cursor` present iff `!done`.
#[derive(Debug, Clone)]
pub struct ImportPage {
    /// Changesets minted this page (previously-linked oids don't count).
    pub imported: u64,
    /// Opaque resume token — a b64url `CursorState` snapshot.
    pub cursor: Option<String>,
    pub done: bool,
    /// Non-commit-peel tip drops (tag→tree/blob) — surfaced, never silent.
    pub skipped_tips: u64,
    /// Where each resolved tip landed (`changeset_id` may still be `null`
    /// mid-page — the link row lands per-oid).
    pub heads: Vec<ImportHead>,
}

/// One resolved import tip — `git_oid` is the PEELED commit oid.
#[derive(Debug, Clone)]
pub struct ImportHead {
    pub name: String,
    pub git_oid: String,
    pub changeset_id: Option<ChangesetId>,
}

/// `git_export` result — carries u7's activity-row fields.
#[derive(Debug, Clone)]
pub struct ExportOutcome {
    pub branch: String,
    /// The minted (or already-present) commit the branch now points at.
    pub git_oid: String,
    /// Commits written this call (`0` on the same-anchor no-op / fast path).
    pub commits_count: u64,
    /// The ref's previous oid (`None` on create) — the `before` half of the
    /// hook-gated move.
    pub before_oid: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal wire shapes (derived-index payloads / cursor)
// ---------------------------------------------------------------------------

/// `vcs:import` `active` row — best-effort claim (BR6.6).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ImportMarker {
    state: String,
    nonce: String,
    started_at: String,
}

/// One resolved tip inside the cursor snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct TipSnapshot {
    name: String,
    git_oid: String,
}

/// Resumable import position — `v` pins the codec version.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CursorState {
    v: u8,
    tips: Vec<TipSnapshot>,
    skip: u64,
    /// Last processed oid — ordering-drift continuity guard on resume.
    last_oid: Option<String>,
}

/// Raw ident bytes for byte-exact re-export (dev 5).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentSidecar {
    a_name_b64: String,
    a_email_b64: String,
    a_unix: i64,
    a_tz: String,
    c_name_b64: String,
    c_email_b64: String,
    c_unix: i64,
    c_tz: String,
}

/// `linked:{cs}` payload — the changeset→git_oid reverse map.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LinkedRow {
    git_oid: String,
}

// ---------------------------------------------------------------------------
// ProtectionChecker — the host-injected verdict port (ADR-006 / TD6.3)
// ---------------------------------------------------------------------------

/// Host-supplied protection verdict port — `oxidean-api` wires the real
/// impl at dispatch time (u7); u6 compiles/tests against mocks. `repo`
/// arrives per call so `oxidean-vcs` stays host-layout-free.
///
/// **No `force` param**: the checker derives intent from `(old, new)`
/// exactly like `check_ref_update` (`new` absent → Delete, `old` absent →
/// Push, else ff-probe → Push|ForcePush) — hook parity is the contract.
#[async_trait::async_trait]
pub trait ProtectionChecker: Send + Sync {
    /// Evaluate whether `actor_capability` may move `refname` `old → new`.
    /// `old`/`new` are git oids; `old = None` is ref creation. Empty
    /// `verdicts.reasons` ⇒ pass; non-empty ⇒ denial. `Err` is an opaque
    /// internal failure — never surfaced to clients.
    async fn check(
        &self,
        repo: &Path,
        refname: &str,
        old: Option<&str>,
        new: &str,
        actor_capability: &str,
    ) -> Result<ProtectionBlockReasons, String>;
}

// ---------------------------------------------------------------------------
// GitInterop
// ---------------------------------------------------------------------------

/// The interop engine — holds the u2–u5 seams plus the two injected traits
/// (`GitBackend` plumbing, host `ProtectionChecker`). `repo` paths arrive
/// per call; the host owns repo-path derivation (TD6.3).
pub struct GitInterop {
    db: Database,
    objects: ObjectStore,
    store: ChangesetStore,
    /// u4 read seam — held for the host's index-backed reads; the export
    /// fold itself runs the stored-manifest + parent-flat overlay (dev 2).
    #[allow(dead_code)]
    index: ReadIndex,
    backend: Arc<dyn GitBackend>,
    checker: Arc<dyn ProtectionChecker>,
    caps: InteropCaps,
}

impl GitInterop {
    /// Caps validate at construction — invalid host config fails closed.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Database,
        objects: ObjectStore,
        store: ChangesetStore,
        index: ReadIndex,
        backend: Arc<dyn GitBackend>,
        checker: Arc<dyn ProtectionChecker>,
        caps: InteropCaps,
    ) -> Result<Self, InteropError> {
        caps.validate(objects.config.max_object_bytes)?;
        Ok(Self {
            db,
            objects,
            store,
            index,
            backend,
            checker,
            caps,
        })
    }

    // ----------------------------------------------------------------------
    // WF1 — explicit import page
    // ----------------------------------------------------------------------

    /// One bounded import page: claim → resolve tips → decode cursor →
    /// `rev_list` window → per-oid import → cursor upsert → release.
    pub async fn import_chain(
        &self,
        repo_id: &str,
        repo: &Path,
        from: Option<&str>,
        cursor: Option<&str>,
        limit: Option<usize>,
    ) -> Result<ImportPage, InteropError> {
        // `Instrument` not `enter()`: the entered guard is `!Send`, which
        // would poison every caller's future (axum handlers need `Send`).
        let span = tracing::info_span!(
            "vcs.git_interop",
            unit = "vcs.git_interop",
            op = "import_chain",
            repo_id
        );
        self.import_chain_inner(repo_id, repo, from, cursor, limit, None)
            .instrument(span)
            .await
    }

    async fn import_chain_inner(
        &self,
        repo_id: &str,
        repo: &Path,
        from: Option<&str>,
        cursor: Option<&str>,
        limit: Option<usize>,
        tips_seed: Option<Vec<TipSnapshot>>,
    ) -> Result<ImportPage, InteropError> {
        if self.db.is_skipped() {
            return Err(warned(InteropError::DbNotConfigured));
        }
        let limit = limit.unwrap_or(self.caps.import_commit_cap);
        if !(1..=self.caps.import_commit_cap).contains(&limit) {
            return Err(invalid("limit", "out of range"));
        }
        if let Some(f) = from {
            if f.starts_with('-') {
                return Err(invalid("from", "must not start with '-'"));
            }
        }

        let nonce = mint_nonce();
        self.claim_import(repo_id, &nonce).await?;
        let out = self
            .import_claimed(repo_id, repo, from, cursor, limit, &nonce, tips_seed)
            .await;
        // Release is nonce-verified and best-effort; the pipeline error wins
        // over a release failure (the marker self-heals via TTL).
        match (out, self.release_import(repo_id, &nonce).await) {
            (Err(e), _) => Err(e),
            (Ok(_), Err(release_err)) => Err(release_err),
            (Ok(page), Ok(())) => Ok(page),
        }
    }

    /// `tips_seed` bypasses `from` resolution entirely (lazy path) — `from`
    /// is honored only on the explicit-import call.
    #[allow(clippy::too_many_arguments)]
    async fn import_claimed(
        &self,
        repo_id: &str,
        repo: &Path,
        from: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
        nonce: &str,
        tips_seed: Option<Vec<TipSnapshot>>,
    ) -> Result<ImportPage, InteropError> {
        // Tip resolution (WF1.4): seed bypasses `from` entirely (lazy path —
        // review3-F7); explicit `from` tries head → tag → raw; no `from`
        // lists heads+tags, peels each to a commit, and drops non-commit
        // peel targets with a warn + count (BR6.28).
        let mut skipped_tips: u64 = 0;
        let resolved = match tips_seed {
            Some(seed) => seed,
            None => match from {
                Some(f) => vec![self.resolve_one_tip(repo, f).await?],
                None => self.resolve_all_tips(repo, &mut skipped_tips).await?,
            },
        };
        if resolved.len() > self.caps.cursor_tips_max {
            return Err(too_large());
        }

        // Cursor decode AFTER tip resolution (SD6.2 ordering) — inbound byte
        // bound first, then version/tips/oid/name/skip field-by-field.
        let mut state = match cursor {
            Some(raw) => self.decode_cursor(raw)?,
            None => CursorState {
                v: 1,
                tips: resolved.clone(),
                skip: 0,
                last_oid: None,
            },
        };
        // The snapshot is authoritative for the walk; freshly resolved tips
        // only drive the `heads` report (skipped_tips counts current drops).
        let walk_oids: Vec<String> = state.tips.iter().map(|t| t.git_oid.clone()).collect();

        let skip = usize::try_from(state.skip)
            .map_err(|_| import_state("cursor: position out of range"))?;
        // `rev-list --max-count` can only cut the NEWEST commits — a paged
        // parents-first window is only well-defined over the FULL bounded
        // list, so the walk always fetches up to `rev_list_max` and an
        // overflow is an honest `TooLarge` (never a mis-sliced resume).
        let rev = self
            .backend
            .rev_list(repo, &walk_oids, self.caps.rev_list_max)
            .await
            .map_err(map_git_err)?;
        if rev.truncated {
            return Err(too_large());
        }
        if skip > rev.oids.len() {
            return Err(import_state("cursor: position beyond the walk"));
        }
        // Continuity: the oid just before the resume position must match —
        // drift means the snapshot no longer describes this walk.
        if skip > 0 {
            let expect = state.last_oid.as_deref().unwrap_or_default();
            if rev.oids.get(skip - 1).map(String::as_str) != Some(expect) {
                return Err(import_state("cursor: continuity check failed"));
            }
        }

        let window: &[String] = &rev.oids[skip..rev.oids.len().min(skip + limit)];
        let mut imported: u64 = 0;
        let mut last_imported_cs: Option<String> = None;
        for oid in window {
            if self
                .db
                .vcs_lookup_import_link(repo_id, oid)
                .await
                .map_err(map_db_err)?
                .is_some()
            {
                continue; // convergence — already imported
            }
            let cs = self.import_one(repo_id, repo, oid).await?;
            self.db
                .vcs_insert_import_link_if_absent(repo_id, oid, cs.as_str())
                .await
                .map_err(map_db_err)?;
            self.upsert_linked(repo_id, cs.as_str(), oid).await?;
            imported += 1;
            last_imported_cs = Some(cs.as_str().to_string());
        }

        let new_skip = skip + window.len();
        let done = new_skip >= rev.oids.len();
        state.skip = new_skip as u64;
        state.last_oid = rev.oids.get(new_skip.saturating_sub(1)).cloned();

        let mut out_cursor = None;
        if !done {
            // Nonce re-verified before the cursor upsert — a lost claim
            // must not write resumable state.
            self.require_claim(repo_id, nonce).await?;
            let encoded = self.encode_cursor(&state)?;
            self.db
                .vcs_upsert_derived_index_rows(
                    repo_id,
                    &[VcsDerivedIndexUpsert {
                        kind: KIND_IMPORT,
                        index_key: KEY_CURSOR,
                        payload: &encoded.json,
                        covering_changeset: last_imported_cs.as_deref(),
                    }],
                )
                .await
                .map_err(map_db_err)?;
            out_cursor = Some(encoded.token);
        }

        let mut heads = Vec::with_capacity(resolved.len());
        for t in &resolved {
            let cs = self
                .db
                .vcs_lookup_import_link(repo_id, &t.git_oid)
                .await
                .map_err(map_db_err)?
                .and_then(|s| ChangesetId::parse(&s).ok());
            heads.push(ImportHead {
                name: t.name.clone(),
                git_oid: t.git_oid.clone(),
                changeset_id: cs,
            });
        }
        Ok(ImportPage {
            imported,
            cursor: out_cursor,
            done,
            skipped_tips,
            heads,
        })
    }

    // ----------------------------------------------------------------------
    // WF2 — lazy single-commit import
    // ----------------------------------------------------------------------

    /// Import the commit (and ancestors) containing `git_oid`, paging until
    /// the oid itself is linked — seeds `import_chain` with the verified tip
    /// directly, bypassing ref resolution (WF2).
    pub async fn lazy_import(
        &self,
        repo_id: &str,
        repo: &Path,
        git_oid: &str,
    ) -> Result<ImportPage, InteropError> {
        let span = tracing::info_span!(
            "vcs.git_interop",
            unit = "vcs.git_interop",
            op = "lazy_import",
            repo_id
        );
        self.lazy_import_inner(repo_id, repo, git_oid)
            .instrument(span)
            .await
    }

    async fn lazy_import_inner(
        &self,
        repo_id: &str,
        repo: &Path,
        git_oid: &str,
    ) -> Result<ImportPage, InteropError> {
        if self.db.is_skipped() {
            return Err(warned(InteropError::DbNotConfigured));
        }
        if !is_git_oid(git_oid) {
            return Err(invalid("git_oid", "must be a 40-hex sha"));
        }
        if self
            .db
            .vcs_lookup_import_link(repo_id, git_oid)
            .await
            .map_err(map_db_err)?
            .is_some()
        {
            return Ok(ImportPage {
                imported: 0,
                cursor: None,
                done: true,
                skipped_tips: 0,
                heads: vec![],
            });
        }
        if self
            .backend
            .verify_commit_oid(repo, git_oid)
            .await
            .map_err(map_git_err)?
            .is_none()
        {
            return Err(not_found());
        }
        let seed = vec![TipSnapshot {
            name: git_oid.to_string(),
            git_oid: git_oid.to_string(),
        }];
        let mut cursor: Option<String> = None;
        let mut total_imported: u64 = 0;
        loop {
            let page = self
                .import_chain_inner(
                    repo_id,
                    repo,
                    None,
                    cursor.as_deref(),
                    None,
                    Some(seed.clone()),
                )
                .await?;
            total_imported += page.imported;
            let done = page.done;
            cursor = page.cursor.clone();
            if self
                .db
                .vcs_lookup_import_link(repo_id, git_oid)
                .await
                .map_err(map_db_err)?
                .is_some()
            {
                return Ok(ImportPage {
                    imported: total_imported,
                    cursor: None,
                    done: true,
                    skipped_tips: page.skipped_tips,
                    heads: page.heads,
                });
            }
            if done {
                // The walk ended without linking the seed — store-state
                // contradiction (the oid verified but never appeared).
                return Err(internal(
                    "lazy import completed without linking the seed oid".into(),
                ));
            }
        }
    }

    // ----------------------------------------------------------------------
    // WF4 — export
    // ----------------------------------------------------------------------

    /// Two-phase export: DFS the changeset DAG to the first verifiable
    /// anchor, mint every missing commit deterministically, check, then ONE
    /// hook-gated ref move. `force` only relaxes the ff requirement — the
    /// protection hook still runs, and a git-only tip is never clobbered.
    pub async fn export(
        &self,
        repo_id: &str,
        repo: &Path,
        head: &str,
        branch: &str,
        force: bool,
        actor_capability: &str,
    ) -> Result<ExportOutcome, InteropError> {
        let span = tracing::info_span!(
            "vcs.git_interop",
            unit = "vcs.git_interop",
            op = "export",
            repo_id
        );
        self.export_inner(repo_id, repo, head, branch, force, actor_capability)
            .instrument(span)
            .await
    }

    async fn export_inner(
        &self,
        repo_id: &str,
        repo: &Path,
        head: &str,
        branch: &str,
        force: bool,
        actor_capability: &str,
    ) -> Result<ExportOutcome, InteropError> {
        if self.db.is_skipped() {
            return Err(warned(InteropError::DbNotConfigured));
        }
        if branch.is_empty() || branch.starts_with('-') {
            return Err(invalid("branch", "must not be empty or dash-led"));
        }
        let refname = format!("refs/heads/{branch}");
        if !self
            .backend
            .check_refname(repo, &refname)
            .await
            .map_err(map_git_err)?
        {
            return Err(invalid("branch", "invalid ref name"));
        }
        if actor_capability.is_empty() {
            return Err(invalid("actor_capability", "required"));
        }
        if ChangesetId::parse(head).is_err() {
            return Err(invalid("head", "must be a changeset id"));
        }
        if self
            .db
            .vcs_get_changeset(repo_id, head)
            .await
            .map_err(map_db_err)?
            .is_none()
        {
            return Err(not_found());
        }

        // Phase 1 — export set: post-order DFS; a linked+verifiable anchor
        // skips the node AND its ancestors (gc'd links count as unlinked).
        let order = self.export_order(repo_id, repo, head).await?;
        let mut seen_blobs: HashMap<String, String> = HashMap::new();
        let mut minted_tip = String::new();
        for cs in &order {
            minted_tip = self.export_one(repo_id, repo, cs, &mut seen_blobs).await?;
        }
        if order.is_empty() {
            minted_tip = self
                .linked_lookup(repo_id, head)
                .await?
                .ok_or_else(|| internal("export: head anchor vanished".into()))?;
        }

        // Existing-tip gate (SD6.5): a git-only tip is never clobbered — not
        // even by force. A linked tip must be an ancestor (changeset-side,
        // with a git-side ff fallback for partially-aligned histories).
        let existing = self
            .backend
            .verify_commit_oid(repo, &refname)
            .await
            .map_err(map_git_err)?;
        let before_oid = existing.clone();
        if existing.as_deref() == Some(minted_tip.as_str()) {
            return Ok(ExportOutcome {
                branch: branch.to_string(),
                git_oid: minted_tip,
                commits_count: order.len() as u64,
                before_oid,
            });
        }
        // An unborn ref is trivially ff; a linked tip must otherwise be an
        // ancestor of `head` (with a git-side probe as the fallback).
        let mut ff_ok = existing.is_none();
        if let Some(tip) = existing.as_deref() {
            let old_cs = self
                .db
                .vcs_lookup_import_link(repo_id, tip)
                .await
                .map_err(map_db_err)?;
            match old_cs {
                None => {
                    return Err(warned(InteropError::RefConflict {
                        refname: refname.clone(),
                        actual: None,
                    }))
                }
                Some(old) => {
                    if self.is_ancestor_cs(repo_id, &old, head).await?
                        || self
                            .backend
                            .is_ancestor(repo, tip, &minted_tip)
                            .await
                            .map_err(map_git_err)?
                    {
                        ff_ok = true;
                    } else if !force {
                        return Err(warned(InteropError::RefConflict {
                            refname: refname.clone(),
                            actual: ChangesetId::parse(&old).ok(),
                        }));
                    }
                }
            }
        }

        // Check BEFORE the single move — verdicts mirror a push denial
        // (AC7.2.2); `force` never reaches the checker.
        let verdicts = self
            .checker
            .check(
                repo,
                &refname,
                existing.as_deref(),
                &minted_tip,
                actor_capability,
            )
            .await
            .map_err(|e| internal(format!("protection check: {e}")))?;
        if !verdicts.reasons.is_empty() {
            return Err(warned(InteropError::Forbidden { verdicts }));
        }

        // Exactly ONE hook-gated move — `update_ref` is forbidden here.
        let move_res = if ff_ok {
            self.backend
                .fast_forward_ref(repo, &refname, &minted_tip, actor_capability)
                .await
        } else {
            self.backend
                .force_update_ref(repo, &refname, &minted_tip, actor_capability)
                .await
        };
        match move_res {
            Ok(()) => Ok(ExportOutcome {
                branch: branch.to_string(),
                git_oid: minted_tip,
                commits_count: order.len() as u64,
                before_oid,
            }),
            Err(GitError::Denied(_)) => Err(warned(InteropError::Forbidden {
                verdicts: hook_denied_verdicts(),
            })),
            Err(GitError::Conflict(_)) => {
                // Tip raced — re-read the linked actual for the conflict body.
                let actual = match self
                    .backend
                    .verify_commit_oid(repo, &refname)
                    .await
                    .map_err(map_git_err)?
                {
                    Some(tip) => self
                        .db
                        .vcs_lookup_import_link(repo_id, &tip)
                        .await
                        .map_err(map_db_err)?
                        .and_then(|s| ChangesetId::parse(&s).ok()),
                    None => None,
                };
                Err(warned(InteropError::RefConflict { refname, actual }))
            }
            Err(other) => Err(map_git_err(other)),
        }
    }

    // ----------------------------------------------------------------------
    // Import internals
    // ----------------------------------------------------------------------

    /// `from` → one peeled commit tip: heads first, then tags, then the raw
    /// rev — `None` of the three resolving ⇒ `NotFound`.
    async fn resolve_one_tip(&self, repo: &Path, from: &str) -> Result<TipSnapshot, InteropError> {
        for rev in [
            format!("refs/heads/{from}"),
            format!("refs/tags/{from}"),
            from.to_string(),
        ] {
            if let Some(oid) = self
                .backend
                .verify_commit_oid(repo, &rev)
                .await
                .map_err(map_git_err)?
            {
                return Ok(TipSnapshot {
                    name: from.to_string(),
                    git_oid: oid,
                });
            }
        }
        Err(not_found())
    }

    /// `None`-`from` resolution: heads+tags peeled to commits; non-commit
    /// peel targets drop + warn + count (BR6.28).
    async fn resolve_all_tips(
        &self,
        repo: &Path,
        skipped: &mut u64,
    ) -> Result<Vec<TipSnapshot>, InteropError> {
        let refs = self.backend.list_refs(repo).await.map_err(map_git_err)?;
        let mut tips = Vec::new();
        for r in refs {
            if !(r.name.starts_with("refs/heads/") || r.name.starts_with("refs/tags/")) {
                continue;
            }
            match self
                .backend
                .verify_commit_oid(repo, &r.oid)
                .await
                .map_err(map_git_err)?
            {
                Some(peeled) => tips.push(TipSnapshot {
                    name: r.name.clone(),
                    git_oid: peeled,
                }),
                None => {
                    *skipped += 1;
                    tracing::warn!(ref_name = %r.name, "import: dropping non-commit tip");
                }
            }
        }
        Ok(tips)
    }

    /// Decode + fully revalidate an inbound cursor (SD6.2) — byte bound
    /// BEFORE base64/JSON, then version, tip count, oid shape, name shape,
    /// and the `skip` bound, field by field.
    fn decode_cursor(&self, raw: &str) -> Result<CursorState, InteropError> {
        if raw.len() > self.caps.cursor_payload_max_bytes {
            return Err(invalid("cursor", "exceeds the payload bound"));
        }
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| import_state("cursor: malformed encoding"))?;
        let state: CursorState = serde_json::from_slice(&bytes)
            .map_err(|_| import_state("cursor: malformed payload"))?;
        if state.v != 1 {
            return Err(import_state("cursor: unsupported version"));
        }
        if state.tips.len() > self.caps.cursor_tips_max {
            return Err(import_state("cursor: too many tips"));
        }
        for t in &state.tips {
            if !is_git_oid(&t.git_oid) {
                return Err(import_state("cursor: malformed tip oid"));
            }
            if t.name.is_empty() || t.name.chars().count() > 256 || t.name.contains('\0') {
                return Err(import_state("cursor: malformed tip name"));
            }
        }
        if state.skip > self.caps.rev_list_max as u64 {
            return Err(import_state("cursor: position out of range"));
        }
        Ok(state)
    }

    /// Serialize + bound-check a cursor; `json` feeds the derived row and
    /// `token` is the opaque client value.
    fn encode_cursor(&self, state: &CursorState) -> Result<EncodedCursor, InteropError> {
        let json =
            serde_json::to_string(state).map_err(|e| internal(format!("cursor encode: {e}")))?;
        if json.len() > self.caps.cursor_payload_max_bytes {
            return Err(too_large());
        }
        Ok(EncodedCursor {
            token: base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json.as_bytes()),
            json,
        })
    }

    /// WF3 — one git commit → one changeset. The refusal inventory runs
    /// BEFORE any object/store write (fail-closed, zero partial state);
    /// sidecars + the reverse `linked:` row land after `create`.
    async fn import_one(
        &self,
        repo_id: &str,
        repo: &Path,
        oid: &str,
    ) -> Result<ChangesetId, InteropError> {
        let raw = self
            .backend
            .cat_commit(repo, oid)
            .await
            .map_err(map_git_err)?;

        // ---- refusal inventory (BR6.23) — pure checks, no I/O ----
        if raw.parents.len() > MAX_PARENTS {
            return Err(invalid("parents", "at most 64 parents"));
        }
        if raw.message.len() > MAX_MESSAGE_BYTES {
            return Err(invalid("message", "exceeds 65535 bytes"));
        }
        let committed_at = unix_to_rfc3339(raw.committer.unix)?;
        unix_to_rfc3339(raw.author.unix)?; // author date must also be valid
        let message = String::from_utf8(raw.message.clone())
            .map_err(|_| invalid("message", "not valid UTF-8"))?;
        let (author_name, author_email) = import_author_parts(&raw.author.name, &raw.author.email)?;
        // Ident sidecar payload is bounded BEFORE create — an un-storable
        // sidecar must fail closed with no changeset.
        let sidecar = IdentSidecar {
            a_name_b64: b64(&raw.author.name),
            a_email_b64: b64(&raw.author.email),
            a_unix: raw.author.unix,
            a_tz: raw.author.tz.clone(),
            c_name_b64: b64(&raw.committer.name),
            c_email_b64: b64(&raw.committer.email),
            c_unix: raw.committer.unix,
            c_tz: raw.committer.tz.clone(),
        };
        let sidecar_json = serde_json::to_string(&sidecar)
            .map_err(|e| internal(format!("sidecar encode: {e}")))?;
        if sidecar_json.len() > self.caps.cursor_payload_max_bytes {
            return Err(invalid("ident", "sidecar exceeds the payload bound"));
        }

        // Parent convergence — the parents-first walk guarantees each git
        // parent already links to a changeset.
        let mut parents_cs = Vec::with_capacity(raw.parents.len());
        for p in &raw.parents {
            let Some(cs) = self
                .db
                .vcs_lookup_import_link(repo_id, p)
                .await
                .map_err(map_db_err)?
            else {
                return Err(internal(
                    "import: parent commit has no changeset link".into(),
                ));
            };
            parents_cs.push(cs);
        }

        // Trees + per-path delta — flat walk bound covers the full leaf
        // listing; the status walk carries `base` (first parent) for the
        // A/M/D/R partition.
        let flat = self
            .backend
            .tree_flat(repo, oid, self.caps.import_tree_max * FLAT_RECORD_AVG)
            .await
            .map_err(map_git_err)?;
        if flat.len() > self.caps.import_tree_max {
            return Err(too_large());
        }
        let mut flat_by_path: BTreeMap<&str, &FlatTreeEntry> = BTreeMap::new();
        for e in &flat {
            flat_by_path.insert(e.path.as_str(), e);
        }
        let status = self
            .backend
            .commit_name_status(
                repo,
                raw.parents.first().map(String::as_str),
                oid,
                self.caps.import_tree_max * STATUS_RECORD_AVG,
            )
            .await
            .map_err(map_git_err)?;
        // Lazily materialized parent flat — only D/R rows need the OLD
        // path's mode (dev-lazy; `commit_name_status` D records name the
        // removed path which the new tree lacks).
        let mut parent_flat: Option<BTreeMap<String, FlatTreeEntry>> = None;

        // Validation loop — all refusals land before any blob streams in.
        struct PendingChange {
            input: FileChangeInput,
            /// (git path to read the blob from, git blob oid) — put-time.
            fetch: Option<(String, u64)>,
        }
        let mut pending: Vec<PendingChange> = Vec::new();
        for row in &status {
            let (kind, path, old_path) = match row.status {
                NameStatusKind::Add => (ChangeKind::Add, row.path.as_str(), None),
                NameStatusKind::Modify | NameStatusKind::TypeChange => {
                    (ChangeKind::Modify, row.path.as_str(), None)
                }
                NameStatusKind::Rename => (
                    ChangeKind::Rename,
                    row.path.as_str(),
                    row.old_path.as_deref(),
                ),
                NameStatusKind::Delete => (ChangeKind::Delete, row.path.as_str(), None),
            };
            // A rename's delete half is folded into the one Rename input —
            // the old path is dropped via `old_path`, matching the manifest
            // grammar (u4 rename rows carry both).
            if kind != ChangeKind::Delete {
                let Some(entry) = flat_by_path.get(path) else {
                    return Err(internal("import: changed path missing from tree".into()));
                };
                if entry.kind == TreeEntryKind::Commit || entry.mode == MODE_GITLINK {
                    // Gitlink/submodule — unrepresentable; drop with a warn
                    // (never silently) rather than minting a wrong mode.
                    tracing::warn!("import: skipping gitlink path in tree");
                    continue;
                }
                let mode = parse_octal_mode(&entry.mode)?;
                if !LEGAL_LEAF_MODES.contains(&mode) {
                    return Err(invalid("mode", "must be one of the pinned mode set"));
                }
                import_validate_path(path)?;
                if entry.size > self.caps.import_blob_max {
                    return Err(too_large());
                }
                let input = FileChangeInput {
                    path: path.to_string(),
                    kind,
                    mode,
                    old_path: old_path.map(str::to_string),
                    blob_hash: None, // filled post-put
                };
                pending.push(PendingChange {
                    input,
                    fetch: Some((path.to_string(), entry.size)),
                });
            } else {
                import_validate_path(path)?;
                if parent_flat.is_none() {
                    parent_flat = Some(self.parent_flat(repo, raw.parents.first()).await?);
                }
                let mode = parent_flat
                    .as_ref()
                    .and_then(|m| m.get(path))
                    .and_then(|e| parse_octal_mode(&e.mode).ok())
                    .filter(|m| LEGAL_LEAF_MODES.contains(m))
                    .unwrap_or(0o100644);
                pending.push(PendingChange {
                    input: FileChangeInput {
                        path: path.to_string(),
                        kind,
                        mode,
                        old_path: None,
                        blob_hash: None,
                    },
                    fetch: None,
                });
            }
        }

        // Blob puts — `cat_blob` is trusted-size bounded (the flat size was
        // checked pre-read); `put` enforces its own streamed cap.
        for p in &mut pending {
            let Some((path, _size)) = &p.fetch else {
                continue;
            };
            let bytes = self
                .backend
                .cat_blob(repo, oid, path)
                .await
                .map_err(map_git_err)?;
            let outcome = self
                .objects
                .put(
                    stream::iter(vec![Ok::<bytes::Bytes, std::convert::Infallible>(
                        bytes::Bytes::from(bytes),
                    )]),
                    repo_id,
                )
                .await
                .map_err(map_object_err)?;
            p.input.blob_hash = Some(outcome.hash);
        }

        let changes: Vec<FileChangeInput> = pending.into_iter().map(|p| p.input).collect();
        let cs = self
            .store
            .create(CreateChangesetInput {
                repo_id: repo_id.to_string(),
                parents: parents_cs,
                author_name,
                author_email,
                message,
                committed_at: Some(committed_at),
                changes,
            })
            .await
            .map_err(map_changeset_err)?;

        self.db
            .vcs_upsert_derived_index_rows(
                repo_id,
                &[
                    VcsDerivedIndexUpsert {
                        kind: KIND_IMPORT,
                        index_key: &format!("ident:{oid}"),
                        payload: &sidecar_json,
                        covering_changeset: Some(cs.as_str()),
                    },
                    VcsDerivedIndexUpsert {
                        kind: KIND_IMPORT,
                        index_key: &format!("ident-cs:{}", cs.as_str()),
                        payload: &sidecar_json,
                        covering_changeset: Some(cs.as_str()),
                    },
                ],
            )
            .await
            .map_err(map_db_err)?;
        Ok(cs)
    }

    /// Parent flat tree for D/R mode recovery — `None` for a root import.
    async fn parent_flat(
        &self,
        repo: &Path,
        first_parent: Option<&String>,
    ) -> Result<BTreeMap<String, FlatTreeEntry>, InteropError> {
        let mut map = BTreeMap::new();
        let Some(p) = first_parent else {
            return Ok(map);
        };
        let flat = self
            .backend
            .tree_flat(repo, p, self.caps.import_tree_max * FLAT_RECORD_AVG)
            .await
            .map_err(map_git_err)?;
        if flat.len() > self.caps.import_tree_max {
            return Err(too_large());
        }
        for e in flat {
            map.insert(e.path.clone(), e);
        }
        Ok(map)
    }

    // ----------------------------------------------------------------------
    // Export internals
    // ----------------------------------------------------------------------

    /// Post-order DFS to the first linked+verifiable anchor — parents emit
    /// before children; a gc'd link counts as unlinked and the walk
    /// continues through it. `visited` counts against `export_chain_max`.
    async fn export_order(
        &self,
        repo_id: &str,
        repo: &Path,
        head: &str,
    ) -> Result<Vec<String>, InteropError> {
        let mut order = Vec::new();
        let mut visited: HashSet<String> = HashSet::new();
        let mut stack: Vec<(String, bool)> = vec![(head.to_string(), false)];
        while let Some((cs, expanded)) = stack.pop() {
            if expanded {
                order.push(cs);
                continue;
            }
            if !visited.insert(cs.clone()) {
                continue;
            }
            if visited.len() > self.caps.export_chain_max {
                return Err(too_large());
            }
            let linked = self.linked_lookup(repo_id, &cs).await?;
            let anchored = match &linked {
                Some(oid) => self
                    .backend
                    .verify_commit_oid(repo, oid)
                    .await
                    .map_err(map_git_err)?
                    .is_some(),
                None => false,
            };
            if anchored {
                continue; // ancestors already exported with this link
            }
            stack.push((cs.clone(), true));
            for p in self
                .db
                .vcs_changeset_parents(repo_id, &cs)
                .await
                .map_err(map_db_err)?
            {
                stack.push((p, false));
            }
        }
        Ok(order)
    }

    /// WF5 — one changeset → one git commit. Base = parents[0]'s git tree;
    /// the stored manifest delta folds over it (mirroring `manifest_diff`
    /// semantics — u4's `index.tree` materialization applies the identical
    /// fold). Blob writes dedup by VCS hash through `seen`.
    async fn export_one(
        &self,
        repo_id: &str,
        repo: &Path,
        cs: &str,
        seen: &mut HashMap<String, String>,
    ) -> Result<String, InteropError> {
        // Gc'd links count as unlinked: only a VERIFIED oid short-circuits —
        // a dangling one falls through and re-mints (the idempotent link
        // insert + `linked:` upsert converge identity onto the fresh oid).
        if let Some(oid) = self.linked_lookup(repo_id, cs).await? {
            if self
                .backend
                .verify_commit_oid(repo, &oid)
                .await
                .map_err(map_git_err)?
                .is_some()
            {
                return Ok(oid);
            }
        }
        let row = self
            .db
            .vcs_get_changeset(repo_id, cs)
            .await
            .map_err(map_db_err)?
            .ok_or_else(not_found)?;
        let parents_cs = self
            .db
            .vcs_changeset_parents(repo_id, cs)
            .await
            .map_err(map_db_err)?;
        let mut parents_git = Vec::with_capacity(parents_cs.len());
        for p in &parents_cs {
            let Some(oid) = self.linked_lookup(repo_id, p).await? else {
                return Err(internal("export: parent changeset has no git link".into()));
            };
            parents_git.push(oid);
        }

        // Fold the stored manifest over parents[0]'s git tree — unchanged
        // entries reuse the parent's blob oid (no rewrite, no read).
        let mut entries: BTreeMap<String, TreeWriteEntry> = BTreeMap::new();
        if let Some(p0) = parents_git.first() {
            let base = self
                .backend
                .tree_flat(repo, p0, self.caps.import_tree_max * FLAT_RECORD_AVG)
                .await
                .map_err(map_git_err)?;
            if base.len() > self.caps.import_tree_max {
                return Err(too_large());
            }
            for e in base {
                if e.kind == TreeEntryKind::Commit {
                    continue; // gitlinks can't re-enter the manifest
                }
                entries.insert(
                    e.path.clone(),
                    TreeWriteEntry {
                        mode: e.mode,
                        oid: e.oid,
                        path: e.path,
                    },
                );
            }
        }
        for ch in self
            .db
            .vcs_list_file_changes(cs)
            .await
            .map_err(map_db_err)?
        {
            match ch.kind.as_str() {
                "delete" => {
                    entries.remove(&ch.path);
                }
                "rename" => {
                    if let Some(old) = &ch.old_path {
                        entries.remove(old);
                    }
                    self.fold_entry(repo, &mut entries, &ch, seen).await?;
                }
                _ => {
                    self.fold_entry(repo, &mut entries, &ch, seen).await?;
                }
            }
        }
        if entries.len() > self.caps.import_tree_max {
            return Err(too_large());
        }
        let leaves: Vec<TreeWriteEntry> = entries.into_values().collect();

        // Idents — sidecar restores the raw import bytes; native changesets
        // fall back to `author → both`, committer time = committed_at UTC.
        let (author, committer) = match self.ident_sidecar_read(repo_id, cs).await? {
            Some((a, c)) => (a, c),
            None => self.fallback_idents(&row)?,
        };

        let oid = self
            .backend
            .write_commit(
                repo,
                &leaves,
                &parents_git,
                &author,
                &committer,
                row.message.as_bytes(),
            )
            .await
            .map_err(map_git_err)?;
        self.db
            .vcs_insert_import_link_if_absent(repo_id, &oid, cs)
            .await
            .map_err(map_db_err)?;
        self.upsert_linked(repo_id, cs, &oid).await?;
        Ok(oid)
    }

    /// Resolve one non-delete manifest row: VCS blob → git oid via `seen`,
    /// else `objects.get` (capped at `export_blob_max + 1`) → `write_object`.
    async fn fold_entry(
        &self,
        repo: &Path,
        entries: &mut BTreeMap<String, TreeWriteEntry>,
        ch: &oxidean_db::VcsFileChangeRow,
        seen: &mut HashMap<String, String>,
    ) -> Result<(), InteropError> {
        let Some(vcs_hash) = ch.blob_hash.as_deref() else {
            return Err(internal("export: non-delete row has no blob".into()));
        };
        let git_oid = match seen.get(vcs_hash) {
            Some(oid) => oid.clone(),
            None => {
                let f = self.objects.get(vcs_hash).await.map_err(map_object_err)?;
                let mut buf = Vec::new();
                let n = f
                    .take(self.caps.export_blob_max + 1)
                    .read_to_end(&mut buf)
                    .await
                    .map_err(|e| internal(format!("export blob read: {e}")))?;
                if n as u64 > self.caps.export_blob_max {
                    return Err(too_large());
                }
                let oid = self
                    .backend
                    .write_object(repo, &buf)
                    .await
                    .map_err(map_git_err)?;
                seen.insert(vcs_hash.to_string(), oid.clone());
                oid
            }
        };
        if entries.len() >= self.caps.import_tree_max && !entries.contains_key(&ch.path) {
            return Err(too_large());
        }
        entries.insert(
            ch.path.clone(),
            TreeWriteEntry {
                mode: format!("{:06o}", ch.mode),
                oid: git_oid,
                path: ch.path.clone(),
            },
        );
        Ok(())
    }

    /// `ident-cs:{cs}` sidecar read-back — `Some` decodes raw ident pairs,
    /// `None` applies the WF5.3 fallback. A PRESENT-but-malformed row is
    /// `Internal` (never silently re-minted).
    async fn ident_sidecar_read(
        &self,
        repo_id: &str,
        cs: &str,
    ) -> Result<Option<(GitIdent, GitIdent)>, InteropError> {
        let rows = self
            .db
            .vcs_query_derived_index(
                repo_id,
                KIND_IMPORT,
                Some(&format!("ident-cs:{cs}")),
                None,
                1,
            )
            .await
            .map_err(map_db_err)?;
        let Some(row) = rows.first() else {
            return Ok(None);
        };
        let sidecar: IdentSidecar = serde_json::from_str(&row.payload)
            .map_err(|e| internal(format!("ident sidecar decode: {e}")))?;
        let decode = |s: &str| {
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| internal(format!("ident sidecar b64: {e}")))
        };
        Ok(Some((
            GitIdent {
                name: decode(&sidecar.a_name_b64)?,
                email: decode(&sidecar.a_email_b64)?,
                unix: sidecar.a_unix,
                tz: sidecar.a_tz,
            },
            GitIdent {
                name: decode(&sidecar.c_name_b64)?,
                email: decode(&sidecar.c_email_b64)?,
                unix: sidecar.c_unix,
                tz: sidecar.c_tz,
            },
        )))
    }

    /// WF5.3 — `author` for both idents, committer date = `committed_at`
    /// normalized UTC (`+0000`). Stored rows are guaranteed well-formed by
    /// `create`, so a parse miss is corruption ⇒ `Internal`.
    fn fallback_idents(
        &self,
        row: &oxidean_db::VcsChangesetRow,
    ) -> Result<(GitIdent, GitIdent), InteropError> {
        let (name, email) = split_composed_author(&row.author)
            .ok_or_else(|| internal("export: stored author fails the composed grammar".into()))?;
        let unix = chrono::DateTime::parse_from_rfc3339(&row.committed_at)
            .map_err(|_| internal("export: stored committed_at is malformed".into()))?
            .timestamp();
        let author = GitIdent {
            name: name.as_bytes().to_vec(),
            email: email.as_bytes().to_vec(),
            unix,
            tz: "+0000".into(),
        };
        Ok((author.clone(), author))
    }

    /// Changeset-DAG ancestry BFS (mirrors u5's private `is_ancestor` over
    /// the same facade — `export_ancestor_probe_max` bounds the mark set).
    async fn is_ancestor_cs(
        &self,
        repo_id: &str,
        anc: &str,
        desc: &str,
    ) -> Result<bool, InteropError> {
        if anc == desc {
            return Ok(true);
        }
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<String> = VecDeque::new();
        visited.insert(desc.to_string());
        queue.push_back(desc.to_string());
        while let Some(x) = queue.pop_front() {
            for p in self
                .db
                .vcs_changeset_parents(repo_id, &x)
                .await
                .map_err(map_db_err)?
            {
                if p == anc {
                    return Ok(true);
                }
                if visited.insert(p.clone()) {
                    if visited.len() > self.caps.export_ancestor_probe_max {
                        return Err(too_large());
                    }
                    queue.push_back(p);
                }
            }
        }
        Ok(false)
    }

    // ----------------------------------------------------------------------
    // Marker + lookup primitives
    // ----------------------------------------------------------------------

    /// `linked:{cs}` row — the changeset→git_oid reverse map (dev 1).
    async fn upsert_linked(
        &self,
        repo_id: &str,
        cs: &str,
        git_oid: &str,
    ) -> Result<(), InteropError> {
        let payload = serde_json::to_string(&LinkedRow {
            git_oid: git_oid.to_string(),
        })
        .map_err(|e| internal(format!("linked encode: {e}")))?;
        self.db
            .vcs_upsert_derived_index_rows(
                repo_id,
                &[VcsDerivedIndexUpsert {
                    kind: KIND_IMPORT,
                    index_key: &format!("linked:{cs}"),
                    payload: &payload,
                    covering_changeset: Some(cs),
                }],
            )
            .await
            .map_err(map_db_err)
    }

    async fn linked_lookup(&self, repo_id: &str, cs: &str) -> Result<Option<String>, InteropError> {
        let rows = self
            .db
            .vcs_query_derived_index(repo_id, KIND_IMPORT, Some(&format!("linked:{cs}")), None, 1)
            .await
            .map_err(map_db_err)?;
        match rows.first() {
            None => Ok(None),
            Some(r) => serde_json::from_str::<LinkedRow>(&r.payload)
                .map(|l| Some(l.git_oid))
                .map_err(|e| internal(format!("linked decode: {e}"))),
        }
    }

    /// Claim the `active` marker — a fresh (non-TTL-expired) claim by
    /// another nonce fails `ImportState`; stale/absent markers are
    /// reclaimed (BR6.6 best-effort mutex).
    async fn claim_import(&self, repo_id: &str, nonce: &str) -> Result<(), InteropError> {
        let rows = self
            .db
            .vcs_query_derived_index(repo_id, KIND_IMPORT, Some(KEY_ACTIVE), None, 1)
            .await
            .map_err(map_db_err)?;
        if let Some(r) = rows.first() {
            if let Ok(m) = serde_json::from_str::<ImportMarker>(&r.payload) {
                if m.state == "active"
                    && marker_fresh(&m.started_at, self.caps.import_lock_ttl_secs)
                {
                    return Err(import_state("import already in progress"));
                }
            }
        }
        self.write_marker(repo_id, "active", nonce).await
    }

    /// Nonce-verified release — only the holder may mark `idle`.
    async fn release_import(&self, repo_id: &str, nonce: &str) -> Result<(), InteropError> {
        let rows = self
            .db
            .vcs_query_derived_index(repo_id, KIND_IMPORT, Some(KEY_ACTIVE), None, 1)
            .await
            .map_err(map_db_err)?;
        let ours = rows
            .first()
            .and_then(|r| serde_json::from_str::<ImportMarker>(&r.payload).ok())
            .is_some_and(|m| m.nonce == nonce);
        if !ours {
            return Ok(());
        }
        self.write_marker(repo_id, "idle", nonce).await
    }

    /// Pre-upsert nonce re-verification — a lost claim must not write
    /// resumable cursor state.
    async fn require_claim(&self, repo_id: &str, nonce: &str) -> Result<(), InteropError> {
        let rows = self
            .db
            .vcs_query_derived_index(repo_id, KIND_IMPORT, Some(KEY_ACTIVE), None, 1)
            .await
            .map_err(map_db_err)?;
        let ours = rows
            .first()
            .and_then(|r| serde_json::from_str::<ImportMarker>(&r.payload).ok())
            .is_some_and(|m| m.state == "active" && m.nonce == nonce);
        if ours {
            Ok(())
        } else {
            Err(import_state("import claim lost"))
        }
    }

    async fn write_marker(
        &self,
        repo_id: &str,
        state: &str,
        nonce: &str,
    ) -> Result<(), InteropError> {
        let payload = serde_json::to_string(&ImportMarker {
            state: state.to_string(),
            nonce: nonce.to_string(),
            started_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        })
        .map_err(|e| internal(format!("marker encode: {e}")))?;
        self.db
            .vcs_upsert_derived_index_rows(
                repo_id,
                &[VcsDerivedIndexUpsert {
                    kind: KIND_IMPORT,
                    index_key: KEY_ACTIVE,
                    payload: &payload,
                    covering_changeset: None,
                }],
            )
            .await
            .map_err(map_db_err)
    }
}

struct EncodedCursor {
    token: String,
    json: String,
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

fn b64(raw: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(raw)
}

fn is_git_oid(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn mint_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = Sha256::new();
    h.update(nanos.to_be_bytes());
    h.update(std::process::id().to_be_bytes());
    let digest = h.finalize();
    hex::encode(&digest[..8])
}

/// `started_at` parses AND lies within `ttl_secs` of now — an unparseable
/// timestamp counts as stale (reclaimable), never a live claim.
fn marker_fresh(started_at: &str, ttl_secs: i64) -> bool {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(started_at) else {
        return false;
    };
    let age = chrono::Utc::now().timestamp() - t.timestamp();
    age <= ttl_secs
}

/// Git ident → store-safe `(name, email)`: control/0x7f bytes sanitize to
/// `?` FIRST, then `<`/`>`/empty/over-255 refuse — mirrored
/// `compose_author` ordering (BR6.23; the value is never echoed).
fn import_author_parts(
    name_raw: &[u8],
    email_raw: &[u8],
) -> Result<(String, String), InteropError> {
    let sanitize = |b: &[u8]| -> String {
        String::from_utf8_lossy(b)
            .chars()
            .map(|c| {
                if c.is_control() || c == '\u{7f}' {
                    '?'
                } else {
                    c
                }
            })
            .collect()
    };
    let name = sanitize(name_raw).trim().to_string();
    let email = sanitize(email_raw);
    if name.is_empty() || email.is_empty() {
        return Err(invalid("author", "name and email are required"));
    }
    for part in [&name, &email] {
        if part.contains('<') || part.contains('>') {
            return Err(invalid("author", "contains '<' or '>'"));
        }
    }
    if format!("{name} <{email}>").chars().count() > MAX_AUTHOR_CHARS {
        return Err(invalid("author", "exceeds 255 characters"));
    }
    Ok((name, email))
}

/// `committer.unix` → RFC3339-seconds UTC — out-of-range values refuse
/// (the refusal inventory's "valid timestamps" arm; git tolerates negatives,
/// the store does not).
fn unix_to_rfc3339(unix: i64) -> Result<String, InteropError> {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|dt| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .ok_or_else(|| invalid("committed_at", "timestamp out of range"))
}

/// Octal mode text → `i64` (git writes `100644`, the store carries
/// `0o100644 = 33188`).
fn parse_octal_mode(mode: &str) -> Result<i64, InteropError> {
    u32::from_str_radix(mode, 8)
        .map(|m| m as i64)
        .map_err(|_| invalid("mode", "not an octal git mode"))
}

/// Import-side path grammar — mirrors `changeset_store::validate_path`
/// (private there); refusal text stays value-free.
fn import_validate_path(path: &str) -> Result<(), InteropError> {
    if path.is_empty() || path.chars().count() > MAX_PATH_CHARS {
        return Err(invalid("path", "length must be 1..=512"));
    }
    if path.starts_with('/') || path.ends_with('/') || path.contains("//") {
        return Err(invalid("path", "bad path shape"));
    }
    if path.bytes().any(|b| b == 0 || b < 0x20 || b == 0x7f) {
        return Err(invalid("path", "contains NUL or control byte"));
    }
    for seg in path.split('/') {
        if seg == "." || seg == ".." || seg == ".git" {
            return Err(invalid("path", "contains a reserved segment"));
        }
    }
    Ok(())
}

/// `Name <email>` → parts at the LAST `<>`, for the export ident fallback.
fn split_composed_author(author: &str) -> Option<(&str, &str)> {
    let lt = author.rfind('<')?;
    let gt = author[lt..].find('>').map(|p| lt + p)?;
    let name = author[..lt].trim();
    let email = &author[lt + 1..gt];
    if name.is_empty() || email.is_empty() {
        return None;
    }
    Some((name, email))
}

// ---------------------------------------------------------------------------
// Tests — live `CliGitBackend` + sqlite fixtures (same lane as u3–u5); the
// only seam mocked is the host `ProtectionChecker` port.
// ---------------------------------------------------------------------------

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::object_store::testutil::{cfg, seed_repo, test_db};
    use crate::object_store::ObjectStore;
    use crate::read_index::ReadIndexCaps;
    use oxidean_git::CliGitBackend;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio::process::Command;

    struct Allow;
    #[async_trait::async_trait]
    impl ProtectionChecker for Allow {
        async fn check(
            &self,
            _repo: &Path,
            _refname: &str,
            _old: Option<&str>,
            _new: &str,
            _cap: &str,
        ) -> Result<ProtectionBlockReasons, String> {
            Ok(ProtectionBlockReasons::default())
        }
    }

    struct Deny;
    #[async_trait::async_trait]
    impl ProtectionChecker for Deny {
        async fn check(
            &self,
            _repo: &Path,
            _refname: &str,
            _old: Option<&str>,
            _new: &str,
            _cap: &str,
        ) -> Result<ProtectionBlockReasons, String> {
            Ok(ProtectionBlockReasons {
                reasons: vec!["policy_denial".into()],
                ..Default::default()
            })
        }
    }

    struct Rig {
        _dbdir: TempDir,
        _odir: TempDir,
        _repodir: TempDir,
        db: Database,
        repo_id: String,
        bare: PathBuf,
        git: CliGitBackend,
        interop: GitInterop,
    }

    async fn rig(checker: Arc<dyn ProtectionChecker>) -> Rig {
        rig_with(checker, |c| c).await
    }

    async fn rig_with(
        checker: Arc<dyn ProtectionChecker>,
        caps: impl FnOnce(InteropCaps) -> InteropCaps,
    ) -> Rig {
        let (dbdir, db) = test_db().await;
        let repo_id = seed_repo(&db, "u-gi", "r-gi").await;
        let odir = tempfile::tempdir().unwrap();
        let objects = ObjectStore::new(db.clone(), cfg(odir.path())).unwrap();
        let store = ChangesetStore::new(db.clone());
        let index = ReadIndex::new(
            db.clone(),
            ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
            ReadIndexCaps::default(),
        );
        let repodir = tempfile::tempdir().unwrap();
        let bare = repodir.path().join("src.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        let max = objects.config.max_object_bytes;
        let interop = GitInterop::new(
            db.clone(),
            objects,
            store,
            index,
            Arc::new(CliGitBackend::new()),
            checker,
            caps(InteropCaps::new(max)),
        )
        .unwrap();
        Rig {
            _dbdir: dbdir,
            _odir: odir,
            _repodir: repodir,
            db,
            repo_id,
            bare,
            git,
            interop,
        }
    }

    /// Sequential-commit fixture — `commit_files` builds on the branch tip
    /// (unborn base → root commit) and pushes through the installed hook
    /// (no-ops without `OXIDEAN_PROTECTION_HELPER`).
    async fn seed(r: &Rig, branch: &str, msg: &str, files: &[(&str, &[u8])]) {
        let changes: Vec<oxidean_git::FileChange> = files
            .iter()
            .map(|(p, b)| oxidean_git::FileChange::Upsert {
                path: p.to_string(),
                content: b.to_vec(),
            })
            .collect();
        r.git
            .commit_files(
                &r.bare,
                branch,
                branch,
                msg,
                &changes,
                "Oxidean",
                "forge@oxidean.local",
                None,
                "admin",
            )
            .await
            .unwrap();
    }

    /// Raw git plumbing for fixtures the trait can't express (tag→blob,
    /// authored idents, gitlinks) — mirrors cli.rs's own test helper.
    async fn git(rig: &Rig, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(&rig.bare)
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    async fn git_out(rig: &Rig, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&rig.bare)
            .args(args)
            .output()
            .await
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    async fn head_oid(rig: &Rig, refname: &str) -> String {
        git_out(rig, &["rev-parse", refname]).await
    }

    // ------------------------------------------------------------------
    // Import
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn import_explicit_from_imports_chain_and_is_idempotent() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a.txt", b"one\n")]).await;
        seed(&r, "main", "c2", &[("b.txt", b"two\n")]).await;

        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        assert_eq!(page.imported, 2);
        assert!(page.done);
        assert!(page.cursor.is_none());
        assert_eq!(page.heads.len(), 1);
        assert!(page.heads[0].changeset_id.is_some());

        let again = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        assert_eq!(again.imported, 0);
        assert!(again.done);
    }

    #[tokio::test]
    async fn import_pages_via_cursor_resume() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        seed(&r, "main", "c2", &[("b", b"2")]).await;
        seed(&r, "main", "c3", &[("c", b"3")]).await;

        let p1 = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, Some(1))
            .await
            .unwrap();
        assert_eq!(p1.imported, 1);
        assert!(!p1.done);
        let cursor = p1.cursor.expect("page 1 emits a cursor");

        let mut imported = p1.imported;
        let mut cursor_opt = Some(cursor);
        let mut last_done = false;
        for _ in 0..6 {
            let p = r
                .interop
                .import_chain(
                    &r.repo_id,
                    &r.bare,
                    Some("main"),
                    cursor_opt.as_deref(),
                    Some(1),
                )
                .await
                .unwrap();
            imported += p.imported;
            cursor_opt = p.cursor;
            last_done = p.done;
            if p.done {
                break;
            }
        }
        assert!(last_done);
        assert_eq!(imported, 3, "3 commits minted across pages");
    }

    #[tokio::test]
    async fn import_all_refs_skips_non_commit_tip() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        // Tag pointing at a blob — non-commit peel → drop + count.
        let blob_oid = git_out(&r, &["hash-object", "-w", "--stdin"]).await;
        let _ = blob_oid; // (written below via hash-object on a file)
        let tmp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(tmp.path(), b"blobby").unwrap();
        let blob = git_out(&r, &["hash-object", "-w", tmp.path().to_str().unwrap()]).await;
        git(&r, &["update-ref", "refs/tags/blobtag", &blob]).await;

        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, None, None, None)
            .await
            .unwrap();
        assert_eq!(page.imported, 1);
        assert_eq!(page.skipped_tips, 1, "blob tag must surface as skipped");
        assert!(page.heads.iter().any(|h| h.name == "refs/heads/main"));
    }

    #[tokio::test]
    async fn import_unresolved_from_is_not_found() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        let e = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("no-such-ref"), None, None)
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::NotFound), "{e:?}");
    }

    #[tokio::test]
    async fn import_validates_from_limit_and_skipped_db() {
        let r = rig(Arc::new(Allow)).await;
        for bad_from in ["-evil", "--upload-pack=x"] {
            let e = r
                .interop
                .import_chain(&r.repo_id, &r.bare, Some(bad_from), None, None)
                .await
                .unwrap_err();
            assert!(matches!(e, InteropError::InvalidInput(_)), "{e:?}");
        }
        for bad_limit in [Some(0), Some(100_001)] {
            let e = r
                .interop
                .import_chain(&r.repo_id, &r.bare, None, None, bad_limit)
                .await
                .unwrap_err();
            assert!(matches!(e, InteropError::InvalidInput(_)), "{e:?}");
        }

        // Skipped db — both surfaces fail closed before touching git.
        let db = Database::skipped();
        let skipped_dir = tempfile::tempdir().unwrap();
        let skipped_cfg = crate::object_store::ObjectStoreConfig {
            objects_dir: skipped_dir.path().to_path_buf(),
            ..Default::default()
        };
        let objects = ObjectStore::new(db.clone(), skipped_cfg.clone()).unwrap();
        let max = objects.config.max_object_bytes;
        let interop = GitInterop::new(
            db.clone(),
            objects,
            ChangesetStore::new(db.clone()),
            ReadIndex::new(
                db.clone(),
                ObjectStore::new(db.clone(), skipped_cfg).unwrap(),
                ReadIndexCaps::default(),
            ),
            Arc::new(CliGitBackend::new()),
            Arc::new(Allow),
            InteropCaps::new(max),
        )
        .unwrap();
        let e = interop
            .import_chain("r", Path::new("/tmp/x"), None, None, None)
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::DbNotConfigured), "{e:?}");
        let e = interop
            .export("r", Path::new("/tmp/x"), "cs_01", "main", false, "push")
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::DbNotConfigured), "{e:?}");
    }

    #[tokio::test]
    async fn import_rejects_forged_and_oversized_cursors() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;

        // Not base64 → ImportState.
        let e = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), Some("!!!"), None)
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::ImportState(_)), "{e:?}");
        // Valid b64, wrong version → ImportState.
        let forged = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"v":9,"tips":[],"skip":0,"last_oid":null}"#);
        let e = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), Some(&forged), None)
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::ImportState(_)), "{e:?}");
        // Oversized raw token → InvalidInput (inbound byte bound first).
        let big = "A".repeat(r.interop.caps.cursor_payload_max_bytes + 1);
        let e = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), Some(&big), None)
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::InvalidInput(_)), "{e:?}");
        // Forged huge skip → ImportState on the skip bound.
        let forged = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(br#"{"v":1,"tips":[],"skip":999999999,"last_oid":null}"#);
        let e = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), Some(&forged), None)
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::ImportState(_)), "{e:?}");
    }

    #[tokio::test]
    async fn import_claim_conflict_and_stale_reclaim() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;

        // Live claim by another nonce → ImportState.
        r.db
            .vcs_upsert_derived_index_rows(
                &r.repo_id,
                &[VcsDerivedIndexUpsert {
                    kind: KIND_IMPORT,
                    index_key: KEY_ACTIVE,
                    payload: r#"{"state":"active","nonce":"other","started_at":"2999-01-01T00:00:00Z"}"#,
                    covering_changeset: None,
                }],
            )
            .await
            .unwrap();
        let e = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::ImportState(_)), "{e:?}");

        // Stale claim → reclaimed, import proceeds.
        r.db
            .vcs_upsert_derived_index_rows(
                &r.repo_id,
                &[VcsDerivedIndexUpsert {
                    kind: KIND_IMPORT,
                    index_key: KEY_ACTIVE,
                    payload: r#"{"state":"active","nonce":"stale","started_at":"2001-01-01T00:00:00Z"}"#,
                    covering_changeset: None,
                }],
            )
            .await
            .unwrap();
        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        assert_eq!(page.imported, 1);
        assert!(page.done);
    }

    #[tokio::test]
    async fn lazy_import_links_seed_and_ancestors() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        seed(&r, "main", "c2", &[("b", b"2")]).await;
        let tip = head_oid(&r, "refs/heads/main").await;

        let page = r
            .interop
            .lazy_import(&r.repo_id, &r.bare, &tip)
            .await
            .unwrap();
        assert_eq!(page.imported, 2);
        assert!(page.done);
        let linked = r.db.vcs_lookup_import_link(&r.repo_id, &tip).await.unwrap();
        assert!(linked.is_some());
        // Second call is the fast path.
        let again = r
            .interop
            .lazy_import(&r.repo_id, &r.bare, &tip)
            .await
            .unwrap();
        assert_eq!(again.imported, 0);
        // Bad shapes + unverifiable oids refuse typed.
        let e = r
            .interop
            .lazy_import(&r.repo_id, &r.bare, "not-a-sha")
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::InvalidInput(_)), "{e:?}");
        let e = r
            .interop
            .lazy_import(&r.repo_id, &r.bare, &"0".repeat(40))
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::NotFound), "{e:?}");
    }

    // ------------------------------------------------------------------
    // Export
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn export_round_trip_reproduces_source_oid() {
        let r = rig(Arc::new(Allow)).await;
        seed(
            &r,
            "main",
            "c1",
            &[("a.txt", b"one\n"), ("dir/b.txt", b"two\n")],
        )
        .await;
        seed(&r, "main", "c2\n\nbody", &[("a.txt", b"one-two\n")]).await;
        let source_tip = head_oid(&r, "refs/heads/main").await;

        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        let head_cs = page.heads[0]
            .changeset_id
            .clone()
            .expect("tip links to a changeset");

        let out = r
            .interop
            .export(&r.repo_id, &r.bare, head_cs.as_str(), "out", false, "push")
            .await
            .unwrap();
        // Source commits already exist + link — nothing to mint.
        assert_eq!(out.commits_count, 0);
        assert_eq!(out.before_oid, None);
        // The deterministic mint must reproduce the EXACT source oid —
        // idents, tree, parents, message all round-trip through the sidecar.
        assert_eq!(out.git_oid, source_tip, "round-trip must be byte-exact");
        assert_eq!(head_oid(&r, "refs/heads/out").await, source_tip);
    }

    #[tokio::test]
    async fn export_same_anchor_is_noop() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        let source_tip = head_oid(&r, "refs/heads/main").await;
        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        let head_cs = page.heads[0].changeset_id.clone().unwrap();

        let first = r
            .interop
            .export(&r.repo_id, &r.bare, head_cs.as_str(), "main", false, "push")
            .await
            .unwrap();
        assert_eq!(first.git_oid, source_tip);
        assert_eq!(first.before_oid.as_deref(), Some(source_tip.as_str()));
    }

    #[tokio::test]
    async fn export_non_ff_refuses_then_force_moves_linked_tip() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        seed(&r, "main", "c2", &[("a", b"2")]).await;
        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        let head_cs = page.heads[0].changeset_id.clone().unwrap();

        // A divergent changeset off the root — its minted tip is NOT a
        // descendant of the current git tip.
        let root_cs =
            r.db.vcs_changeset_parents(&r.repo_id, head_cs.as_str())
                .await
                .unwrap()[0]
                .clone();
        let out = r
            .interop
            .objects
            .put(
                stream::iter(vec![Ok::<bytes::Bytes, std::convert::Infallible>(
                    bytes::Bytes::from_static(b"divergent\n"),
                )]),
                &r.repo_id,
            )
            .await
            .unwrap();
        let div = r
            .interop
            .store
            .create(CreateChangesetInput {
                repo_id: r.repo_id.clone(),
                parents: vec![root_cs],
                author_name: "T".into(),
                author_email: "t@e.com".into(),
                message: "divergent".into(),
                committed_at: Some("2026-01-02T03:04:05Z".into()),
                changes: vec![FileChangeInput {
                    path: "div.txt".into(),
                    kind: ChangeKind::Add,
                    mode: 33188,
                    old_path: None,
                    blob_hash: Some(out.hash),
                }],
            })
            .await
            .unwrap();

        let e = r
            .interop
            .export(&r.repo_id, &r.bare, div.as_str(), "main", false, "push")
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::RefConflict { .. }), "{e:?}");
        // The tip never moved.
        assert_eq!(
            head_oid(&r, "refs/heads/main").await,
            git_out(&r, &["rev-parse", "refs/heads/main"]).await
        );

        // Force on a LINKED tip passes the gate and moves the ref.
        let out = r
            .interop
            .export(&r.repo_id, &r.bare, div.as_str(), "main", true, "push")
            .await
            .unwrap();
        assert_eq!(head_oid(&r, "refs/heads/main").await, out.git_oid);
    }

    #[tokio::test]
    async fn export_never_clobbers_git_only_tip() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        let head_cs = page.heads[0].changeset_id.clone().unwrap();

        // A git-side commit lands AFTER import — unlinked tip.
        seed(&r, "other", "git-only", &[("z", b"z")]).await;

        for force in [false, true] {
            let e = r
                .interop
                .export(
                    &r.repo_id,
                    &r.bare,
                    head_cs.as_str(),
                    "other",
                    force,
                    "push",
                )
                .await
                .unwrap_err();
            match e {
                InteropError::RefConflict { actual, .. } => {
                    assert!(actual.is_none(), "git-only tip reports null actual")
                }
                other => panic!("expected RefConflict, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn export_checker_denial_happens_before_any_move() {
        let r = rig(Arc::new(Deny)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        let head_cs = page.heads[0].changeset_id.clone().unwrap();

        let e = r
            .interop
            .export(
                &r.repo_id,
                &r.bare,
                head_cs.as_str(),
                "denied",
                false,
                "push",
            )
            .await
            .unwrap_err();
        match e {
            InteropError::Forbidden { verdicts } => {
                assert_eq!(verdicts.reasons, vec!["policy_denial".to_string()]);
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        // Ref must not exist — the check ran before the one allowed move.
        let out = Command::new("git")
            .arg("-C")
            .arg(&r.bare)
            .args(["rev-parse", "--verify", "refs/heads/denied"])
            .output()
            .await
            .unwrap();
        assert!(
            !out.status.success(),
            "denied export must not create the ref"
        );
    }

    #[tokio::test]
    async fn export_validates_branch_capability_and_head() {
        let r = rig(Arc::new(Allow)).await;
        seed(&r, "main", "c1", &[("a", b"1")]).await;
        let page = r
            .interop
            .import_chain(&r.repo_id, &r.bare, Some("main"), None, None)
            .await
            .unwrap();
        let head_cs = page.heads[0].changeset_id.clone().unwrap();

        for bad_branch in ["", "-x", "bad..name", "bad name"] {
            let e = r
                .interop
                .export(
                    &r.repo_id,
                    &r.bare,
                    head_cs.as_str(),
                    bad_branch,
                    false,
                    "push",
                )
                .await
                .unwrap_err();
            assert!(
                matches!(e, InteropError::InvalidInput(_)),
                "{bad_branch}: {e:?}"
            );
        }
        let e = r
            .interop
            .export(&r.repo_id, &r.bare, head_cs.as_str(), "ok", false, "")
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::InvalidInput(_)), "{e:?}");
        let e = r
            .interop
            .export(&r.repo_id, &r.bare, "not-a-cs", "ok", false, "push")
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::InvalidInput(_)), "{e:?}");
        let e = r
            .interop
            .export(
                &r.repo_id,
                &r.bare,
                "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR",
                "ok",
                false,
                "push",
            )
            .await
            .unwrap_err();
        assert!(matches!(e, InteropError::NotFound), "{e:?}");
    }

    #[tokio::test]
    async fn export_native_changeset_uses_fallback_idents() {
        let r = rig(Arc::new(Allow)).await;
        // A changeset minted natively (no import sidecar) — fallback idents
        // = stored author + committed_at.
        let out = r
            .interop
            .objects
            .put(
                stream::iter(vec![Ok::<bytes::Bytes, std::convert::Infallible>(
                    bytes::Bytes::from_static(b"native\n"),
                )]),
                &r.repo_id,
            )
            .await
            .unwrap();
        let cs = r
            .interop
            .store
            .create(CreateChangesetInput {
                repo_id: r.repo_id.clone(),
                parents: vec![],
                author_name: "N Tive".into(),
                author_email: "nt@e.com".into(),
                message: "native cs".into(),
                committed_at: Some("2026-03-04T05:06:07Z".into()),
                changes: vec![FileChangeInput {
                    path: "n.txt".into(),
                    kind: ChangeKind::Add,
                    mode: 33188,
                    old_path: None,
                    blob_hash: Some(out.hash),
                }],
            })
            .await
            .unwrap();
        let out = r
            .interop
            .export(&r.repo_id, &r.bare, cs.as_str(), "n", false, "push")
            .await
            .unwrap();
        let author = git_out(&r, &["log", "-1", "--format=%an|%ae|%ct", "refs/heads/n"]).await;
        assert_eq!(author, "N Tive|nt@e.com|1772600767");
        let _ = out;
    }

    #[tokio::test]
    async fn caps_validate_rejects_bad_host_config() {
        let (dbdir, db) = test_db().await;
        let _seed = seed_repo(&db, "u-c", "r-c").await;
        let odir = tempfile::tempdir().unwrap();
        let objects = ObjectStore::new(db.clone(), cfg(odir.path())).unwrap();
        let max = objects.config.max_object_bytes;
        let _ = dbdir;
        for bad in [
            InteropCaps {
                import_commit_cap: 0,
                ..InteropCaps::new(max)
            },
            InteropCaps {
                import_blob_max: max + 1,
                ..InteropCaps::new(max)
            },
            InteropCaps {
                export_blob_max: 0,
                ..InteropCaps::new(max)
            },
            InteropCaps {
                rev_list_max: 0,
                ..InteropCaps::new(max)
            },
        ] {
            let res = GitInterop::new(
                db.clone(),
                ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
                ChangesetStore::new(db.clone()),
                ReadIndex::new(
                    db.clone(),
                    ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
                    ReadIndexCaps::default(),
                ),
                Arc::new(CliGitBackend::new()),
                Arc::new(Allow),
                bad,
            );
            match res {
                Err(InteropError::InvalidInput(_)) => {}
                other => panic!("expected InvalidInput, got {:?}", other.map(|_| ())),
            }
        }
    }

    // ------------------------------------------------------------------
    // Wire bridge
    // ------------------------------------------------------------------

    #[test]
    fn to_app_error_maps_every_variant() {
        assert_eq!(
            to_app_error(&InteropError::DbNotConfigured).code,
            "db.not_configured"
        );
        assert_eq!(to_app_error(&InteropError::NotFound).code, "vcs.not_found");
        assert_eq!(
            to_app_error(&InteropError::InvalidInput("x".into())).code,
            "vcs.invalid_input"
        );
        assert_eq!(
            to_app_error(&InteropError::ImportState("x".into())).code,
            "vcs.import_state"
        );
        assert_eq!(
            to_app_error(&InteropError::Forbidden {
                verdicts: ProtectionBlockReasons::default()
            })
            .code,
            "vcs.forbidden"
        );
        assert_eq!(
            to_app_error(&InteropError::RefConflict {
                refname: "r".into(),
                actual: None
            })
            .code,
            "vcs.ref_conflict"
        );
        assert_eq!(to_app_error(&InteropError::TooLarge).code, "vcs.too_large");
        assert_eq!(
            to_app_error(&InteropError::QuotaExceeded(QuotaScope::Object)).code,
            "vcs.quota_exceeded"
        );
        assert_eq!(
            to_app_error(&InteropError::Internal("x".into())).code,
            "internal"
        );
    }

    #[test]
    fn git_error_bridge_maps_every_variant() {
        assert!(matches!(
            map_git_err(GitError::Denied("hook".into())),
            InteropError::Forbidden { .. }
        ));
        assert!(matches!(
            map_git_err(GitError::Conflict("moved".into())),
            InteropError::RefConflict { .. }
        ));
        assert!(matches!(
            map_git_err(GitError::NotFound("x".into())),
            InteropError::NotFound
        ));
        assert!(matches!(
            map_git_err(GitError::InvalidArg("x".into())),
            InteropError::InvalidInput(_)
        ));
        assert!(matches!(
            map_git_err(GitError::Process(STDOUT_BOUND_SENTINEL.into())),
            InteropError::TooLarge
        ));
        assert!(matches!(
            map_git_err(GitError::Process("other".into())),
            InteropError::Internal(_)
        ));
        assert!(matches!(
            map_git_err(GitError::Io(std::io::Error::other("x"))),
            InteropError::Internal(_)
        ));
    }

    #[test]
    fn small_helpers_behave() {
        assert!(is_git_oid(&"a".repeat(40)));
        assert!(!is_git_oid(&"a".repeat(39)));
        assert!(!is_git_oid(&"g".repeat(40)));
        assert_eq!(parse_octal_mode("100644").unwrap(), 33188);
        assert!(parse_octal_mode("xyz").is_err());
        assert_eq!(split_composed_author("A N <a@b>").unwrap(), ("A N", "a@b"));
        assert!(split_composed_author("no email").is_none());
        assert!(unix_to_rfc3339(1_772_588_777).is_ok());
        // Git tolerates negative author times — they normalize to 1969;
        // only unrepresentable extremes refuse.
        assert_eq!(unix_to_rfc3339(-1).unwrap(), "1969-12-31T23:59:59Z");
        assert!(unix_to_rfc3339(i64::MAX).is_err());
        // import_author_parts: sanitize-then-refuse ordering.
        let (n, e) = import_author_parts(b"A\tN", b"a@b").unwrap();
        assert_eq!(n, "A?N");
        assert_eq!(e, "a@b");
        assert!(import_author_parts(b"", b"a@b").is_err());
        assert!(import_author_parts(b"A<B", b"a@b").is_err());
        for p in ["/abs", "a//b", "a/./b", "a/../b", ".git/x", "a\u{1}b"] {
            assert!(import_validate_path(p).is_err(), "{p}");
        }
        assert!(import_validate_path("a/b/c.rs").is_ok());
    }
}
