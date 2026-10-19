//! `vcs.*` procedure orchestration (contract C1) — the single place host
//! dispatch arms call after auth + repo resolution. Two-case flag gate:
//! `vcs.repo_enable`/`vcs.status` run flag-free (they ARE the discovery
//! surface); every other procedure requires an enabled repo surface flag.
//!
//! Error policy: every domain error lands on the frozen C1 taxonomy
//! (`VcsErrorCode`); `Internal`/io/db detail is opaque `"internal"` — never a
//! raw error string on the wire. `DbNotConfigured` surfaces
//! `db.not_configured` (the u6 interop bridge convention).

use std::path::Path;

use base64::Engine as _;
use bytes::Bytes;
use futures_util::{stream, FutureExt};
use oxidean_core::{
    AppError, ChangesetId, ManifestEntry, ManifestEntryKind, ObjectId, VcsAuthor, VcsBlameEntry,
    VcsBlameRequest, VcsBlameResponse, VcsChangesetCreateRequest, VcsChangesetCreateResponse,
    VcsChangesetPublic, VcsDiffEntry, VcsDiffRequest, VcsDiffResponse, VcsDiffTotals, VcsErrorCode,
    VcsFileGetRequest, VcsFileGetResponse, VcsFileMode, VcsGitExportRequest, VcsGitExportResponse,
    VcsGitImportHead, VcsGitImportRequest, VcsGitImportResponse, VcsHeadPublic, VcsLogRequest,
    VcsLogResponse, VcsMergeBaseRequest, VcsMergeBaseResponse, VcsMergeRequest, VcsMergeResponse,
    VcsPathHistoryEntry, VcsPathHistoryRequest, VcsPathHistoryResponse, VcsRebaseRequest,
    VcsRebaseResponse, VcsRefListRequest, VcsRefListResponse, VcsRefPublic, VcsRefUpdateRequest,
    VcsRefUpdateResponse, VcsRepoEnableResponse, VcsStatusResponse, VcsTreeEntry, VcsTreeRequest,
    VcsTreeResponse, VCS_ACTIVITY_CHANGESET_CREATE, VCS_ACTIVITY_GIT_EXPORT,
    VCS_ACTIVITY_GIT_IMPORT, VCS_ACTIVITY_MERGE, VCS_ACTIVITY_REBASE, VCS_ACTIVITY_REF_UPDATE,
    VCS_ACTIVITY_REPO_ENABLE,
};
use oxidean_db::Database;
use tokio::io::AsyncReadExt;

use crate::activity::{ActivityRecord, ActivityRecorder};
use crate::changeset_store::{
    is_git_oid, ChangeKind, ChangesetError, ChangesetRecord, ChangesetStore, CreateChangesetInput,
    FileChangeInput,
};
use crate::git_interop::{self, GitInterop};
use crate::merge_engine::{MergeEngine, MergeError, MergeOutcome};
use crate::object_store::{ObjectStore, ObjectStoreError};
use crate::read_index::{DiffKind, IndexError, ReadIndex};
use crate::ref_registry::{RefError, RefRegistry};

/// Inline-content transport cap (contract `limits.inline` — 256 KiB decoded).
const INLINE_CONTENT_MAX: usize = 256 * 1024;
/// Pagination ceilings — `limits.pagination` ≤ 500 per call.
const LOG_PAGE_MAX: usize = 500;
const REFS_PAGE_MAX: usize = 500;
const DIFF_PAGE_MAX: usize = 500;
const TREE_PAGE_MAX: usize = 500;
const BLAME_PAGE_MAX: usize = 500;
const PATH_HISTORY_PAGE_MAX: usize = 500;
/// Default page size when the request omits `limit`.
const DEFAULT_PAGE: usize = 50;

/// Assembled engine stack — the host builds each piece (three `ObjectStore`
/// handles over one directory is the established pattern; the store carries
/// no mutable state) and injects `GitInterop` when the git backend +
/// protection checker are wired.
pub struct VcsApi {
    db: Database,
    store: ChangesetStore,
    refs: RefRegistry,
    index: ReadIndex,
    merge: MergeEngine,
    objects: ObjectStore,
    interop: Option<GitInterop>,
    activity: ActivityRecorder,
}

impl VcsApi {
    pub fn new(
        db: Database,
        objects: ObjectStore,
        index: ReadIndex,
        merge: MergeEngine,
        interop: Option<GitInterop>,
    ) -> Self {
        Self {
            db: db.clone(),
            store: ChangesetStore::new(db.clone()),
            refs: RefRegistry::new(db.clone()),
            index,
            merge,
            objects,
            interop,
            activity: ActivityRecorder::new(db),
        }
    }

    /// The object store for `/api/vcs/{owner}/{name}/objects` routes and the
    /// GC job (`put`/`get`/`verify`/`gc_once`/`clear_stale_lock`).
    pub fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    // ------------------------------------------------------------------
    // Flag gate (C1 two-case)
    // ------------------------------------------------------------------

    /// The repo's flag row (`None` = never enabled). A skipped db fails
    /// closed; a facade error is opaque.
    async fn flag_row(
        &self,
        repo_id: &str,
    ) -> Result<Option<oxidean_db::VcsRepoSurfaceFlagRow>, AppError> {
        if self.db.is_skipped() {
            return Err(db_not_configured());
        }
        self.db
            .vcs_get_repo_surface_flag(repo_id)
            .await
            .map_err(|_| internal())
    }

    /// Write gate: the flag row must exist AND be enabled (C1 — "absent flag
    /// row denies all; enabled:false denies writes only").
    async fn require_enabled(&self, repo_id: &str) -> Result<(), AppError> {
        match self.flag_row(repo_id).await? {
            Some(flag) if flag.enabled => Ok(()),
            _ => Err(VcsErrorCode::Disabled.app_error("vcs is not enabled on this repository")),
        }
    }

    /// Read gate: any flag row serves reads — even `enabled:false` (AC1.2.1);
    /// only a never-enabled repo (absent row) denies.
    async fn require_flag_row(&self, repo_id: &str) -> Result<(), AppError> {
        match self.flag_row(repo_id).await? {
            Some(_) => Ok(()),
            None => Err(VcsErrorCode::Disabled.app_error("vcs is not enabled on this repository")),
        }
    }

    /// Interop is host-injected — `vcs.git_import`/`vcs.git_export` (and the
    /// lazy-import retry on git-oid read misses) need it.
    fn require_interop(&self) -> Result<&GitInterop, AppError> {
        self.interop
            .as_ref()
            .ok_or_else(|| VcsErrorCode::Disabled.app_error("git interop is not configured"))
    }

    // ------------------------------------------------------------------
    // Procedures (C1)
    // ------------------------------------------------------------------

    /// `vcs.repo_enable` — toggle the repo flag. Flag-free by design: this IS
    /// the switch. Admin-only upstream (the handler resolves `Admin`).
    pub async fn repo_enable(
        &self,
        repo_id: &str,
        actor_id: &str,
        enabled: bool,
    ) -> Result<VcsRepoEnableResponse, AppError> {
        if self.db.is_skipped() {
            return Err(db_not_configured());
        }
        self.db
            .vcs_upsert_repo_surface_flag(repo_id, enabled, Some(actor_id))
            .await
            .map_err(|_| internal())?;
        self.activity
            .record(
                repo_id,
                actor_id,
                ActivityRecord {
                    push_type: VCS_ACTIVITY_REPO_ENABLE,
                    ..Default::default()
                },
            )
            .await;
        Ok(VcsRepoEnableResponse { enabled })
    }

    /// `vcs.status` — the flag-row gate with a wrinkle: a never-enabled repo
    /// (absent row) fails `vcs.disabled` outright; a present row reports its
    /// `enabled` bit plus heads/counts regardless of the bit (enabled:false
    /// still serves reads, AC1.2.1).
    pub async fn status(&self, repo_id: &str) -> Result<VcsStatusResponse, AppError> {
        let flag = match self.flag_row(repo_id).await? {
            Some(f) => f,
            None => {
                return Err(
                    VcsErrorCode::Disabled.app_error("vcs is not enabled on this repository")
                )
            }
        };
        let mut heads = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let (page, next) = self
                .refs
                .list(repo_id, cursor.as_deref(), REFS_PAGE_MAX)
                .await
                .map_err(map_ref_err)?;
            for r in &page {
                if let Some(id) = &r.changeset_id {
                    heads.push(VcsHeadPublic {
                        name: r.name.clone(),
                        changeset_id: id.clone(),
                    });
                }
            }
            match next {
                Some(c) => cursor = Some(c),
                None => break,
            }
        }
        let changeset_count = self
            .db
            .vcs_count_changesets(repo_id)
            .await
            .map_err(|_| internal())?
            .max(0) as u64;
        let counts = self
            .db
            .vcs_repo_object_counts(repo_id)
            .await
            .map_err(|_| internal())?;
        Ok(VcsStatusResponse {
            enabled: flag.enabled,
            heads,
            changeset_count,
            object_count: counts.object_count.max(0) as u64,
            storage_bytes: counts.storage_bytes.max(0) as u64,
        })
    }

    /// `vcs.changeset_create` — resolve inline `content_b64` entries into
    /// objects first (≤256 KiB each), then `ChangesetStore::create` (canonical
    /// identity + idempotent single tx).
    pub async fn changeset_create(
        &self,
        repo_id: &str,
        actor_id: &str,
        actor: &VcsAuthor,
        req: VcsChangesetCreateRequest,
    ) -> Result<VcsChangesetCreateResponse, AppError> {
        self.require_enabled(repo_id).await?;
        let author = req.author.unwrap_or_else(|| actor.clone());
        let mut changes = Vec::with_capacity(req.changes.len());
        for entry in &req.changes {
            changes.push(self.resolve_entry(repo_id, entry).await?);
        }
        let id = self
            .store
            .create(CreateChangesetInput {
                repo_id: repo_id.to_string(),
                parents: req.parents.iter().map(|p| p.as_str().to_string()).collect(),
                author_name: author.name,
                author_email: author.email,
                message: req.message.clone(),
                committed_at: None,
                changes,
            })
            .await
            .map_err(map_changeset_err)?;
        let rec = self
            .store
            .get(repo_id, id.as_str())
            .await
            .map_err(map_changeset_err)?;
        self.activity
            .record(
                repo_id,
                actor_id,
                ActivityRecord {
                    push_type: VCS_ACTIVITY_CHANGESET_CREATE,
                    after_oid: Some(id.as_str().to_string()),
                    commits_count: Some(1),
                    commit_message: Some(rec.message.clone()),
                    ..Default::default()
                },
            )
            .await;
        Ok(VcsChangesetCreateResponse {
            changeset_id: id,
            manifest_hash: rec.manifest_hash,
        })
    }

    /// `vcs.changeset_get` — `id` accepts a changeset id or a git oid; a
    /// git-oid link miss triggers `lazy_import` and re-resolves once (AC7.1.2).
    pub async fn changeset_get(
        &self,
        repo_id: &str,
        repo: &Path,
        id: &str,
    ) -> Result<VcsChangesetPublic, AppError> {
        self.require_flag_row(repo_id).await?;
        if crate::changeset_store::is_changeset_id(id) {
            let rec = self
                .store
                .get(repo_id, id)
                .await
                .map_err(map_changeset_err)?;
            return changeset_public(&rec);
        }
        if is_git_oid(id) {
            if let Some(cs) = self.resolve_git_oid(repo_id, repo, id).await? {
                let rec = self
                    .store
                    .get(repo_id, cs.as_str())
                    .await
                    .map_err(map_changeset_err)?;
                return changeset_public(&rec);
            }
        }
        Err(VcsErrorCode::NotFound.app_error("vcs: not found"))
    }

    /// `vcs.log` — `from` is `ref_name | changeset_id | git_oid`; a git-oid
    /// link miss lazy-imports and retries once.
    pub async fn log(
        &self,
        repo_id: &str,
        repo: &Path,
        req: VcsLogRequest,
    ) -> Result<VcsLogResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let limit = page_limit(req.limit, LOG_PAGE_MAX);
        let first = self
            .index
            .log(repo_id, &req.from, limit, req.cursor.as_deref())
            .await;
        let (entries, cursor) = match first {
            Ok(page) => page,
            Err(IndexError::NotFound) if is_git_oid(&req.from) => {
                self.lazy(repo_id, repo, &req.from).await?;
                self.index
                    .log(repo_id, &req.from, limit, req.cursor.as_deref())
                    .await
                    .map_err(map_index_err)?
            }
            Err(e) => return Err(map_index_err(e)),
        };
        let mut out = Vec::with_capacity(entries.len());
        for rec in &entries {
            out.push(changeset_public(rec)?);
        }
        Ok(VcsLogResponse {
            entries: out,
            cursor,
        })
    }

    /// `vcs.diff` — paginated file diff; per-file patch text is `None` here
    /// (the inline patch materialization lives on the web surface — the RPC
    /// carries the entry shape + totals).
    pub async fn diff(
        &self,
        repo_id: &str,
        req: VcsDiffRequest,
    ) -> Result<VcsDiffResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let limit = page_limit(req.limit, DIFF_PAGE_MAX);
        let (entries, cursor) = self
            .index
            .diff(
                repo_id,
                req.from.as_str(),
                req.to.as_str(),
                req.paths.as_deref(),
                limit,
                req.cursor.as_deref(),
            )
            .await
            .map_err(map_index_err)?;
        let mut out = Vec::with_capacity(entries.len());
        let mut totals = VcsDiffTotals {
            files: 0,
            additions: 0,
            deletions: 0,
        };
        for e in &entries {
            totals.files += 1;
            out.push(VcsDiffEntry {
                path: e.path.clone(),
                kind: diff_kind_to_wire(e.kind),
                old_mode: e.old_mode.map(mode_to_wire).transpose()?,
                new_mode: e.new_mode.map(mode_to_wire).transpose()?,
                old_blob: e.old_blob.as_deref().map(object_id).transpose()?,
                new_blob: e.new_blob.as_deref().map(object_id).transpose()?,
                patch: None,
            });
        }
        Ok(VcsDiffResponse {
            entries: out,
            cursor,
            totals,
        })
    }

    /// `vcs.tree` — materialized tree listing at a changeset.
    pub async fn tree(
        &self,
        repo_id: &str,
        req: VcsTreeRequest,
    ) -> Result<VcsTreeResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let limit = page_limit(req.limit, TREE_PAGE_MAX);
        let (entries, cursor) = self
            .index
            .tree(
                repo_id,
                req.changeset_id.as_str(),
                req.path_prefix.as_deref(),
                limit,
                req.cursor.as_deref(),
            )
            .await
            .map_err(map_index_err)?;
        let mut out = Vec::with_capacity(entries.len());
        for e in &entries {
            out.push(VcsTreeEntry {
                path: e.path.clone(),
                mode: mode_to_wire(e.mode)?,
                blob: object_id(&e.blob_hash)?,
            });
        }
        Ok(VcsTreeResponse {
            entries: out,
            cursor,
        })
    }

    /// `vcs.file_get` — blob bytes inline when ≤ the 256 KiB cap, else
    /// `too_large: true` and `content_b64: null` (fetch via the object route).
    pub async fn file_get(
        &self,
        repo_id: &str,
        req: VcsFileGetRequest,
    ) -> Result<VcsFileGetResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let entry = self
            .index
            .file_at(repo_id, req.changeset_id.as_str(), &req.path)
            .await
            .map_err(map_index_err)?;
        let size = self
            .db
            .vcs_get_object(&entry.blob_hash)
            .await
            .map_err(|_| internal())?
            .map(|row| row.size.max(0) as u64)
            .unwrap_or(0);
        let too_large = size as usize > INLINE_CONTENT_MAX;
        let content_b64 = if too_large {
            None
        } else {
            let mut file = self
                .objects
                .get(&entry.blob_hash)
                .await
                .map_err(map_object_err)?;
            let mut buf = Vec::with_capacity(size as usize);
            file.read_to_end(&mut buf).await.map_err(|_| internal())?;
            Some(base64::engine::general_purpose::STANDARD.encode(&buf))
        };
        Ok(VcsFileGetResponse {
            blob: object_id(&entry.blob_hash)?,
            mode: mode_to_wire(entry.mode)?,
            size,
            content_b64,
            too_large,
        })
    }

    /// `vcs.blame` — `at` omitted resolves `main`'s tip, else the sole head.
    pub async fn blame(
        &self,
        repo_id: &str,
        req: VcsBlameRequest,
    ) -> Result<VcsBlameResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let at = match &req.at {
            Some(id) => id.clone(),
            None => self.resolve_default_head(repo_id).await?,
        };
        let limit = page_limit(req.limit, BLAME_PAGE_MAX);
        let (lines, cursor) = self
            .index
            .blame(
                repo_id,
                &req.path,
                at.as_str(),
                limit,
                req.cursor.as_deref(),
            )
            .await
            .map_err(map_index_err)?;
        let mut out = Vec::with_capacity(lines.len());
        for l in &lines {
            out.push(VcsBlameEntry {
                changeset_id: l.changeset_id.clone(),
                author: parse_author(&l.author),
                content: String::from_utf8_lossy(&l.content).into_owned(),
            });
        }
        Ok(VcsBlameResponse {
            entries: out,
            cursor,
        })
    }

    /// `vcs.path_history` — chronological touches of `path`.
    pub async fn path_history(
        &self,
        repo_id: &str,
        req: VcsPathHistoryRequest,
    ) -> Result<VcsPathHistoryResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let limit = page_limit(req.limit, PATH_HISTORY_PAGE_MAX);
        let (touches, cursor) = self
            .index
            .path_history(repo_id, &req.path, limit, req.cursor.as_deref())
            .await
            .map_err(map_index_err)?;
        let out = touches
            .iter()
            .map(|t| VcsPathHistoryEntry {
                changeset_id: t.changeset_id.clone(),
                kind: change_kind_to_wire(t.kind),
                path: t.old_path.clone().unwrap_or_else(|| t.path.clone()),
                committed_at: t.committed_at.clone(),
            })
            .collect();
        Ok(VcsPathHistoryResponse {
            entries: out,
            cursor,
        })
    }

    /// `vcs.merge_base` — the derived-index base; `BaseNotFound` maps to the
    /// `vcs.no_common_ancestor` code.
    pub async fn merge_base(
        &self,
        repo_id: &str,
        req: VcsMergeBaseRequest,
    ) -> Result<VcsMergeBaseResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let base = self
            .index
            .merge_base(repo_id, req.a.as_str(), req.b.as_str())
            .await
            .map_err(map_index_err)?;
        Ok(VcsMergeBaseResponse { merge_base: base })
    }

    /// `vcs.merge` — `a` is ours/first-parent, `b` theirs; the engine's
    /// ancestor fast-path yields the descendant with `fast_forward: true`.
    pub async fn merge(
        &self,
        repo_id: &str,
        actor_id: &str,
        actor: &VcsAuthor,
        req: VcsMergeRequest,
    ) -> Result<VcsMergeResponse, AppError> {
        self.require_enabled(repo_id).await?;
        let outcome = self
            .merge
            .merge(
                repo_id,
                req.a.as_str(),
                req.b.as_str(),
                &actor.name,
                &actor.email,
                req.message.clone(),
            )
            .await
            .map_err(map_merge_err)?;
        let (head, ff) = match &outcome {
            MergeOutcome::FastForward { head } => (head.clone(), Some(true)),
            MergeOutcome::Merged { changeset_id } => (changeset_id.clone(), None),
        };
        self.activity
            .record(
                repo_id,
                actor_id,
                ActivityRecord {
                    push_type: VCS_ACTIVITY_MERGE,
                    after_oid: Some(head.as_str().to_string()),
                    commits_count: Some(if ff.is_some() { 0 } else { 1 }),
                    commit_message: req.message.clone(),
                    ..Default::default()
                },
            )
            .await;
        Ok(VcsMergeResponse {
            changeset_id: head,
            fast_forward: ff,
        })
    }

    /// `vcs.rebase` — replays ONE changeset onto `new_parent_id`.
    pub async fn rebase(
        &self,
        repo_id: &str,
        actor_id: &str,
        actor: &VcsAuthor,
        req: VcsRebaseRequest,
    ) -> Result<VcsRebaseResponse, AppError> {
        self.require_enabled(repo_id).await?;
        let outcome = self
            .merge
            .rebase(
                repo_id,
                req.changeset_id.as_str(),
                req.new_parent_id.as_str(),
                &actor.name,
                &actor.email,
                req.message.clone(),
            )
            .await
            .map_err(map_merge_err)?;
        let head = match &outcome {
            MergeOutcome::FastForward { head } => head.clone(),
            MergeOutcome::Merged { changeset_id } => changeset_id.clone(),
        };
        self.activity
            .record(
                repo_id,
                actor_id,
                ActivityRecord {
                    push_type: VCS_ACTIVITY_REBASE,
                    after_oid: Some(head.as_str().to_string()),
                    commits_count: Some(1),
                    commit_message: req.message.clone(),
                    ..Default::default()
                },
            )
            .await;
        Ok(VcsRebaseResponse { head })
    }

    /// `vcs.ref_list` — all named refs, paginated.
    pub async fn ref_list(
        &self,
        repo_id: &str,
        req: VcsRefListRequest,
    ) -> Result<VcsRefListResponse, AppError> {
        self.require_flag_row(repo_id).await?;
        let limit = page_limit(req.limit, REFS_PAGE_MAX);
        let (refs, cursor) = self
            .refs
            .list(repo_id, req.cursor.as_deref(), limit)
            .await
            .map_err(map_ref_err)?;
        let mut out = Vec::with_capacity(refs.len());
        for r in &refs {
            if let Some(id) = &r.changeset_id {
                out.push(VcsRefPublic {
                    name: r.name.clone(),
                    changeset_id: id.clone(),
                    updated_at: r.updated_at.clone(),
                });
            }
        }
        Ok(VcsRefListResponse {
            entries: out,
            cursor,
        })
    }

    /// `vcs.ref_update` — CAS create/move/delete. `expected_old: null` is
    /// create-only; `new: null` is delete; absent fields never reach serde
    /// (`rpc.bad_input` at the DTO boundary).
    pub async fn ref_update(
        &self,
        repo_id: &str,
        actor_id: &str,
        req: VcsRefUpdateRequest,
    ) -> Result<VcsRefUpdateResponse, AppError> {
        self.require_enabled(repo_id).await?;
        let expected = req.expected_old.value().map(|id| id.as_str().to_string());
        let new = req.new.value().map(|id| id.as_str().to_string());
        let r = self
            .refs
            .cas(repo_id, &req.ref_name, expected.as_deref(), new.as_deref())
            .await
            .map_err(map_ref_err)?;
        self.activity
            .record(
                repo_id,
                actor_id,
                ActivityRecord {
                    push_type: VCS_ACTIVITY_REF_UPDATE,
                    ref_name: Some(req.ref_name.clone()),
                    before_oid: expected,
                    after_oid: new,
                    ..Default::default()
                },
            )
            .await;
        Ok(VcsRefUpdateResponse {
            ref_name: r.name,
            changeset_id: r.changeset_id,
        })
    }

    /// `vcs.git_import` — paged chain import; resumable via the opaque cursor.
    pub async fn git_import(
        &self,
        repo_id: &str,
        repo: &Path,
        actor_id: &str,
        req: VcsGitImportRequest,
    ) -> Result<VcsGitImportResponse, AppError> {
        self.require_enabled(repo_id).await?;
        let page = self
            .require_interop()?
            .import_chain(
                repo_id,
                repo,
                req.from.as_deref(),
                req.cursor.as_deref(),
                req.limit.map(|n| n.max(0) as usize),
            )
            .await
            .map_err(|e| git_interop::to_app_error(&e))?;
        // C4: after_oid = the last imported changeset_id the page reports.
        let after = page
            .heads
            .iter()
            .rev()
            .find_map(|h| h.changeset_id.as_ref().map(|id| id.as_str().to_string()));
        self.activity
            .record(
                repo_id,
                actor_id,
                ActivityRecord {
                    push_type: VCS_ACTIVITY_GIT_IMPORT,
                    after_oid: after,
                    commits_count: Some(page.imported as i64),
                    ..Default::default()
                },
            )
            .await;
        Ok(VcsGitImportResponse {
            imported: page.imported,
            cursor: page.cursor,
            done: page.done,
            skipped_tips: page.skipped_tips,
            heads: page
                .heads
                .into_iter()
                .map(|h| VcsGitImportHead {
                    name: h.name,
                    git_oid: h.git_oid,
                    changeset_id: h.changeset_id,
                })
                .collect(),
        })
    }

    /// `vcs.git_export` — mint the deterministic commit chain then move
    /// `refs/heads/{branch}` through ONE hook-gated ref update.
    pub async fn git_export(
        &self,
        repo_id: &str,
        repo: &Path,
        actor_id: &str,
        actor_capability: &str,
        req: VcsGitExportRequest,
    ) -> Result<VcsGitExportResponse, AppError> {
        self.require_enabled(repo_id).await?;
        let outcome = self
            .require_interop()?
            .export(
                repo_id,
                repo,
                req.head.as_str(),
                &req.branch,
                req.force.unwrap_or(false),
                actor_capability,
            )
            .await
            .map_err(|e| git_interop::to_app_error(&e))?;
        self.activity
            .record(
                repo_id,
                actor_id,
                ActivityRecord {
                    push_type: VCS_ACTIVITY_GIT_EXPORT,
                    ref_name: Some(format!("refs/heads/{}", outcome.branch)),
                    before_oid: outcome.before_oid.clone(),
                    after_oid: Some(outcome.git_oid.clone()),
                    commits_count: Some(outcome.commits_count as i64),
                    ..Default::default()
                },
            )
            .await;
        Ok(VcsGitExportResponse {
            branch: outcome.branch,
            git_oid: outcome.git_oid,
        })
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /// One `content_b64`/`blob` manifest entry → `FileChangeInput`. Inline
    /// content decodes (≤256 KiB) and lands in the object store; `blob`
    /// references an existing object. `delete` carries neither; `rename`
    /// carries `old_path`.
    async fn resolve_entry(
        &self,
        repo_id: &str,
        entry: &ManifestEntry,
    ) -> Result<FileChangeInput, AppError> {
        let kind = wire_kind_to_domain(entry.kind);
        let mut blob_hash: Option<String> = None;
        match kind {
            ChangeKind::Delete => {
                if entry.blob.is_some() || entry.content_b64.is_some() {
                    return Err(invalid("a delete entry carries no blob or content"));
                }
            }
            _ => match (&entry.blob, &entry.content_b64) {
                (Some(_), Some(_)) => {
                    return Err(invalid("carry exactly one of blob or content_b64"));
                }
                (Some(oid), None) => {
                    blob_hash = Some(
                        oid.as_str()
                            .strip_prefix("sha256:")
                            .ok_or_else(internal)?
                            .to_string(),
                    );
                }
                (None, Some(b64)) => {
                    let bytes = base64::engine::general_purpose::STANDARD
                        .decode(b64)
                        .map_err(|_| invalid("content_b64 is not valid base64"))?;
                    if bytes.len() > INLINE_CONTENT_MAX {
                        return Err(
                            VcsErrorCode::TooLarge.app_error("inline content exceeds 256 KiB")
                        );
                    }
                    let stream =
                        stream::iter(vec![Ok::<Bytes, std::io::Error>(Bytes::from(bytes))]);
                    let outcome = self
                        .objects
                        .put(stream, repo_id)
                        .await
                        .map_err(map_object_err)?;
                    blob_hash = Some(outcome.hash);
                }
                (None, None) => {
                    return Err(invalid("a non-delete entry requires blob or content_b64"));
                }
            },
        }
        Ok(FileChangeInput {
            path: entry.path.clone(),
            kind,
            mode: mode_from_wire(entry.mode),
            old_path: entry.old_path.clone(),
            blob_hash,
        })
    }

    /// Git-oid → changeset resolution with the lazy-import retry: probe the
    /// link, on miss `lazy_import` the chain then probe again (once).
    async fn resolve_git_oid(
        &self,
        repo_id: &str,
        repo: &Path,
        git_oid: &str,
    ) -> Result<Option<ChangesetId>, AppError> {
        if let Some(cs) = self
            .store
            .resolve_git_oid(repo_id, git_oid)
            .await
            .map_err(map_changeset_err)?
        {
            return Ok(Some(cs));
        }
        self.lazy(repo_id, repo, git_oid).await?;
        self.store
            .resolve_git_oid(repo_id, git_oid)
            .await
            .map_err(map_changeset_err)
    }

    /// `lazy_import` when interop is wired; absent interop means the oid
    /// simply has no changeset (miss stays a miss — never a Disabled error on
    /// a read path).
    async fn lazy(&self, repo_id: &str, repo: &Path, git_oid: &str) -> Result<(), AppError> {
        if let Some(interop) = self.interop.as_ref() {
            interop
                // `boxed()` enforces `Send` on the interop future AND cuts the
                // coroutine-witness chain rustc unrolls for `Send` checks
                // (dispatch → handler → api → interop is deep enough to trip
                // `recursion_depth_exceeding_limit` unboxed).
                .lazy_import(repo_id, repo, git_oid)
                .boxed()
                .await
                .map_err(|e| git_interop::to_app_error(&e))?;
        }
        Ok(())
    }

    /// `main`'s tip when it exists; otherwise the sole head when exactly one
    /// ref resolves; else `vcs.invalid_input` (ambiguous `at`).
    async fn resolve_default_head(&self, repo_id: &str) -> Result<ChangesetId, AppError> {
        match self.refs.resolve(repo_id, "main").await {
            Ok(id) => return Ok(id),
            Err(RefError::DbNotConfigured) => return Err(db_not_configured()),
            Err(RefError::Internal(_)) => return Err(internal()),
            Err(_) => {}
        }
        let (refs, _) = self
            .refs
            .list(repo_id, None, 2)
            .await
            .map_err(map_ref_err)?;
        let heads: Vec<ChangesetId> = refs.iter().filter_map(|r| r.changeset_id.clone()).collect();
        match heads.len() {
            // Contract: no refs → `vcs.not_found`; ≥2 refs without `main` →
            // `vcs.invalid_input` (the caller must pass `at` explicitly).
            0 => Err(VcsErrorCode::NotFound.app_error("vcs: no refs exist")),
            1 => Ok(heads.into_iter().next().unwrap()),
            _ => Err(invalid(
                "`at` is required when `main` is absent and heads are ambiguous",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Wire mapping helpers
// ---------------------------------------------------------------------------

fn page_limit(limit: Option<i64>, max: usize) -> usize {
    match limit {
        Some(n) if n > 0 => (n as usize).min(max),
        _ => DEFAULT_PAGE,
    }
}

fn object_id(bare: &str) -> Result<ObjectId, AppError> {
    ObjectId::parse(&format!("sha256:{bare}")).map_err(|_| internal())
}

fn mode_to_wire(mode: i64) -> Result<VcsFileMode, AppError> {
    match mode {
        0o100644 => Ok(VcsFileMode::Regular),
        0o100755 => Ok(VcsFileMode::Executable),
        0o120000 => Ok(VcsFileMode::Symlink),
        _ => Err(internal()),
    }
}

fn mode_from_wire(mode: VcsFileMode) -> i64 {
    match mode {
        VcsFileMode::Regular => 0o100644,
        VcsFileMode::Executable => 0o100755,
        VcsFileMode::Symlink => 0o120000,
    }
}

fn change_kind_to_wire(kind: ChangeKind) -> ManifestEntryKind {
    match kind {
        ChangeKind::Add => ManifestEntryKind::Add,
        ChangeKind::Modify => ManifestEntryKind::Modify,
        ChangeKind::Delete => ManifestEntryKind::Delete,
        ChangeKind::Rename => ManifestEntryKind::Rename,
    }
}

fn wire_kind_to_domain(kind: ManifestEntryKind) -> ChangeKind {
    match kind {
        ManifestEntryKind::Add => ChangeKind::Add,
        ManifestEntryKind::Modify => ChangeKind::Modify,
        ManifestEntryKind::Delete => ChangeKind::Delete,
        ManifestEntryKind::Rename => ChangeKind::Rename,
    }
}

fn diff_kind_to_wire(kind: DiffKind) -> ManifestEntryKind {
    match kind {
        DiffKind::Add => ManifestEntryKind::Add,
        DiffKind::Delete => ManifestEntryKind::Delete,
        DiffKind::Modify => ManifestEntryKind::Modify,
    }
}

/// Stored `Name <email>` → `{name, email}`; a shape violation degrades to
/// `{name: composed, email: ""}` rather than failing the read (the write path
/// guarantees the composed shape).
fn parse_author(composed: &str) -> VcsAuthor {
    match composed.rsplit_once(" <") {
        Some((name, rest)) if rest.ends_with('>') && rest.len() > 1 => VcsAuthor {
            name: name.to_string(),
            email: rest[..rest.len() - 1].to_string(),
        },
        _ => VcsAuthor {
            name: composed.to_string(),
            email: String::new(),
        },
    }
}

fn changeset_public(rec: &ChangesetRecord) -> Result<VcsChangesetPublic, AppError> {
    let mut manifest = Vec::with_capacity(rec.manifest.len());
    for c in &rec.manifest {
        manifest.push(ManifestEntry {
            path: c.path.clone(),
            kind: change_kind_to_wire(c.kind),
            mode: mode_to_wire(c.mode)?,
            old_path: c.old_path.clone(),
            blob: c.blob_hash.as_deref().map(object_id).transpose()?,
            content_b64: None,
        });
    }
    Ok(VcsChangesetPublic {
        changeset_id: rec.id.clone(),
        parents: rec.parents.clone(),
        author: parse_author(&rec.author),
        message: rec.message.clone(),
        committed_at: rec.committed_at.clone(),
        manifest,
    })
}

// ---------------------------------------------------------------------------
// Error bridges — every variant lands on the C1 taxonomy; `internal` and
// `db.not_configured` carry no raw detail.
// ---------------------------------------------------------------------------

fn internal() -> AppError {
    AppError::new("internal", "internal error")
}

fn db_not_configured() -> AppError {
    AppError::new("db.not_configured", "vcs database not configured")
}

fn invalid(msg: impl Into<String>) -> AppError {
    VcsErrorCode::InvalidInput.app_error(format!("vcs: {}", msg.into()))
}

fn map_changeset_err(e: ChangesetError) -> AppError {
    match e {
        ChangesetError::ParentNotFound => {
            VcsErrorCode::NotFound.app_error("vcs: parent changeset not found")
        }
        ChangesetError::ObjectNotFound { .. } => {
            VcsErrorCode::NotFound.app_error("vcs: object not found")
        }
        ChangesetError::ChangesetNotFound => {
            VcsErrorCode::NotFound.app_error("vcs: changeset not found")
        }
        ChangesetError::InvalidInput(reason) => invalid(reason),
        ChangesetError::Internal(_) => internal(),
        ChangesetError::DbNotConfigured => db_not_configured(),
    }
}

fn map_index_err(e: IndexError) -> AppError {
    match e {
        IndexError::NotFound => VcsErrorCode::NotFound.app_error("vcs: not found"),
        IndexError::PathNotFound => VcsErrorCode::NotFound.app_error("vcs: path not found"),
        IndexError::BaseNotFound => {
            VcsErrorCode::NoCommonAncestor.app_error("vcs: no common ancestor")
        }
        IndexError::FrontierTooLarge => {
            VcsErrorCode::TooLarge.app_error("vcs: walk bound exceeded")
        }
        IndexError::CursorMalformed => invalid("malformed cursor"),
        IndexError::InvalidInput(reason) => invalid(reason),
        IndexError::Internal(_) => internal(),
        IndexError::DbNotConfigured => db_not_configured(),
    }
}

fn map_ref_err(e: RefError) -> AppError {
    match e {
        RefError::RefNotFound => VcsErrorCode::ref_conflict("vcs: ref conflict", None),
        RefError::RefConflict { actual } => {
            VcsErrorCode::ref_conflict("vcs: ref conflict", ChangesetId::parse(&actual).ok())
        }
        RefError::TargetNotFound => {
            VcsErrorCode::NotFound.app_error("vcs: target changeset not found")
        }
        RefError::InvalidInput(reason) => invalid(reason),
        RefError::Internal(_) => internal(),
        RefError::DbNotConfigured => db_not_configured(),
    }
}

fn map_merge_err(e: MergeError) -> AppError {
    match e {
        MergeError::NotFound => VcsErrorCode::NotFound.app_error("vcs: not found"),
        MergeError::InvalidInput(reason) => invalid(reason),
        MergeError::NoCommonAncestor => {
            VcsErrorCode::NoCommonAncestor.app_error("vcs: no common ancestor")
        }
        MergeError::TooLarge => VcsErrorCode::TooLarge.app_error("vcs: bound exceeded"),
        MergeError::QuotaExceeded(_) => {
            VcsErrorCode::QuotaExceeded.app_error("vcs: quota exceeded")
        }
        MergeError::Conflict(entries) => VcsErrorCode::conflict("vcs: merge conflict", entries),
        MergeError::DbNotConfigured => db_not_configured(),
        MergeError::Internal(_) => internal(),
    }
}

fn map_object_err(e: ObjectStoreError) -> AppError {
    match e {
        ObjectStoreError::QuotaExceeded(_) => {
            VcsErrorCode::QuotaExceeded.app_error("vcs: quota exceeded")
        }
        ObjectStoreError::NotFound => VcsErrorCode::NotFound.app_error("vcs: object not found"),
        ObjectStoreError::InvalidHash => invalid("invalid object hash"),
        ObjectStoreError::Stream(_) => invalid("object stream failed"),
        ObjectStoreError::DbNotConfigured => db_not_configured(),
        ObjectStoreError::Config(_)
        | ObjectStoreError::Db(_)
        | ObjectStoreError::Io(_)
        | ObjectStoreError::Internal(_) => internal(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changeset_store::ChangesetStore;
    use crate::git_interop::{InteropCaps, ProtectionChecker};
    use crate::merge_engine::MergeCaps;
    use crate::object_store::testutil::{cfg, seed_repo, test_db};
    use crate::read_index::ReadIndexCaps;
    use oxidean_core::{Nullable, ProtectionBlockReasons};
    use oxidean_git::{CliGitBackend, GitBackend};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    const CS_FILE: &[u8] = b"hello vcs\n";
    const OBJ_SHA256: &str = "sha256";

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
        api: VcsApi,
    }

    async fn rig(with_interop: bool) -> Rig {
        rig_with_check(with_interop, Arc::new(Allow)).await
    }

    async fn rig_with_check(with_interop: bool, checker: Arc<dyn ProtectionChecker>) -> Rig {
        let (dbdir, db) = test_db().await;
        let repo_id = seed_repo(&db, "u-api", "r-api").await;
        let odir = tempfile::tempdir().unwrap();
        let objects = ObjectStore::new(db.clone(), cfg(odir.path())).unwrap();
        let index = ReadIndex::new(
            db.clone(),
            ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
            ReadIndexCaps::default(),
        );
        let merge = MergeEngine::new(
            db.clone(),
            ReadIndex::new(
                db.clone(),
                ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
                ReadIndexCaps::default(),
            ),
            ChangesetStore::new(db.clone()),
            ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
            MergeCaps::default(),
        )
        .unwrap();
        let repodir = tempfile::tempdir().unwrap();
        let bare = repodir.path().join("src.git");
        let git = CliGitBackend::new();
        git.init_bare(&bare, "main").await.unwrap();
        let interop = if with_interop {
            let max = objects.config.max_object_bytes;
            Some(
                GitInterop::new(
                    db.clone(),
                    ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
                    ChangesetStore::new(db.clone()),
                    ReadIndex::new(
                        db.clone(),
                        ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
                        ReadIndexCaps::default(),
                    ),
                    Arc::new(CliGitBackend::new()),
                    checker,
                    InteropCaps::new(max),
                )
                .unwrap(),
            )
        } else {
            None
        };
        let api = VcsApi::new(db.clone(), objects, index, merge, interop);
        Rig {
            _dbdir: dbdir,
            _odir: odir,
            _repodir: repodir,
            db,
            repo_id,
            bare,
            git,
            api,
        }
    }

    async fn enable(r: &Rig) {
        r.api.repo_enable(&r.repo_id, "u-api", true).await.unwrap();
    }

    fn actor() -> VcsAuthor {
        VcsAuthor {
            name: "U Api".into(),
            email: "u-api@example.com".into(),
        }
    }

    fn entry(path: &str, body: &[u8]) -> ManifestEntry {
        ManifestEntry {
            path: path.to_string(),
            kind: ManifestEntryKind::Add,
            mode: VcsFileMode::Regular,
            old_path: None,
            blob: None,
            content_b64: Some(base64::engine::general_purpose::STANDARD.encode(body)),
        }
    }

    fn create_req(
        parents: Vec<ChangesetId>,
        message: &str,
        changes: Vec<ManifestEntry>,
    ) -> VcsChangesetCreateRequest {
        VcsChangesetCreateRequest {
            owner: "u-api".into(),
            name: "r-api".into(),
            parents,
            message: message.to_string(),
            author: None,
            changes,
        }
    }

    async fn seed_cs(r: &Rig, message: &str, path: &str, body: &[u8]) -> ChangesetId {
        r.api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![], message, vec![entry(path, body)]),
            )
            .await
            .unwrap()
            .changeset_id
    }

    async fn seed_git(r: &Rig, msg: &str, files: &[(&str, &[u8])]) {
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
                "main",
                "main",
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

    fn err_code(e: &AppError) -> String {
        e.code.clone()
    }

    // ---------------- flag gate (two-case) ----------------

    #[tokio::test]
    async fn flag_gate_disabled_then_enabled() {
        let r = rig(false).await;
        // Never-enabled repo: reads AND writes deny; status errors too.
        let err = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![], "x", vec![entry("a", b"a")]),
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.disabled");
        let err = r
            .api
            .ref_list(
                &r.repo_id,
                VcsRefListRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.disabled");
        let err = r.api.status(&r.repo_id).await.unwrap_err();
        assert_eq!(err_code(&err), "vcs.disabled");
        // repo_enable is flag-free and flips the gate.
        let res = r.api.repo_enable(&r.repo_id, "u-api", true).await.unwrap();
        assert!(res.enabled);
        let st = r.api.status(&r.repo_id).await.unwrap();
        assert!(st.enabled);
        // enabled:false → reads still serve (AC1.2.1), writes deny.
        r.api.repo_enable(&r.repo_id, "u-api", false).await.unwrap();
        let refs = r
            .api
            .ref_list(
                &r.repo_id,
                VcsRefListRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert!(refs.entries.is_empty());
        let st = r.api.status(&r.repo_id).await.unwrap();
        assert!(!st.enabled);
        let err = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![], "x", vec![entry("a", b"a")]),
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.disabled");
    }

    #[tokio::test]
    async fn repo_enable_records_activity() {
        let r = rig(false).await;
        r.api.repo_enable(&r.repo_id, "u-api", true).await.unwrap();
        let (rows, _) =
            r.db.list_repo_activity(&r.repo_id, None, None, 0, 10)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].push_type, VCS_ACTIVITY_REPO_ENABLE);
        assert!(rows[0].ref_name.is_none());
    }

    // ---------------- create / get / log ----------------

    #[tokio::test]
    async fn changeset_create_inline_and_get() {
        let r = rig(false).await;
        enable(&r).await;
        let id = seed_cs(&r, "first", "a.txt", CS_FILE).await;
        let got = r
            .api
            .changeset_get(&r.repo_id, &r.bare, id.as_str())
            .await
            .unwrap();
        assert_eq!(got.changeset_id, id);
        assert_eq!(got.author.name, "U Api");
        assert_eq!(got.message, "first");
        assert_eq!(got.manifest.len(), 1);
        assert_eq!(got.manifest[0].path, "a.txt");
        // read rows carry `blob`, never inline content.
        let oid = got.manifest[0].blob.clone().unwrap();
        assert!(oid.as_str().starts_with(OBJ_SHA256));
        assert!(got.manifest[0].content_b64.is_none());
        // activity row for the create.
        let (rows, _) =
            r.db.list_repo_activity(&r.repo_id, None, None, 0, 10)
                .await
                .unwrap();
        assert_eq!(
            rows.iter()
                .filter(|x| x.push_type == VCS_ACTIVITY_CHANGESET_CREATE)
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn changeset_create_both_blob_and_content_rejects() {
        let r = rig(false).await;
        enable(&r).await;
        let mut e = entry("a", b"a");
        e.blob = Some(
            ObjectId::parse(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .unwrap(),
        );
        let err = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![], "x", vec![e]),
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.invalid_input");
    }

    #[tokio::test]
    async fn changeset_create_inline_over_cap_is_too_large() {
        let r = rig(false).await;
        enable(&r).await;
        let big = vec![b'x'; INLINE_CONTENT_MAX + 1];
        let err = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![], "x", vec![entry("big", &big)]),
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.too_large");
    }

    #[tokio::test]
    async fn changeset_get_unknown_id_not_found() {
        let r = rig(false).await;
        enable(&r).await;
        let err = r
            .api
            .changeset_get(&r.repo_id, &r.bare, "cs_01H8V4JGZ0K2B9Q7X3F5M6NPQR")
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.not_found");
        // garbage-shaped id also not found (not invalid_input — `id` is a lookup key).
        let err = r
            .api
            .changeset_get(&r.repo_id, &r.bare, "not-an-id")
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.not_found");
    }

    #[tokio::test]
    async fn log_from_ref_and_pagination() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "one", "a", b"1").await;
        let b = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![a.clone()], "two", vec![entry("b", b"2")]),
            )
            .await
            .unwrap()
            .changeset_id;
        r.api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Null,
                    new: Nullable::Value(b),
                },
            )
            .await
            .unwrap();
        let page = r
            .api
            .log(
                &r.repo_id,
                &r.bare,
                VcsLogRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    from: "main".into(),
                    limit: Some(1),
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(page.entries.len(), 1);
        assert!(page.cursor.is_some());
        let page2 = r
            .api
            .log(
                &r.repo_id,
                &r.bare,
                VcsLogRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    from: "main".into(),
                    limit: Some(1),
                    cursor: page.cursor.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(page2.entries.len(), 1);
        assert_ne!(page2.entries[0].changeset_id, page.entries[0].changeset_id);
    }

    #[tokio::test]
    async fn log_malformed_cursor_is_invalid_input() {
        let r = rig(false).await;
        enable(&r).await;
        seed_cs(&r, "one", "a", b"1").await;
        let err = r
            .api
            .log(
                &r.repo_id,
                &r.bare,
                VcsLogRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    from: "main".into(),
                    limit: None,
                    cursor: Some("not-a-cursor".into()),
                },
            )
            .await
            .unwrap_err();
        // `main` doesn't exist → the from-resolution error fires first.
        assert!(matches!(
            err_code(&err).as_str(),
            "vcs.not_found" | "vcs.invalid_input"
        ));
    }

    // ---------------- reads: diff/tree/file_get/blame/path_history ----------------

    #[tokio::test]
    async fn diff_tree_file_get_round_trip() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "one", "dir/f.txt", b"line one\n").await;
        let b = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(
                    vec![a.clone()],
                    "two",
                    vec![ManifestEntry {
                        path: "dir/f.txt".into(),
                        kind: ManifestEntryKind::Modify,
                        mode: VcsFileMode::Regular,
                        old_path: None,
                        blob: None,
                        content_b64: Some(
                            base64::engine::general_purpose::STANDARD.encode(b"line two\n"),
                        ),
                    }],
                ),
            )
            .await
            .unwrap()
            .changeset_id;

        let tree = r
            .api
            .tree(
                &r.repo_id,
                VcsTreeRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    changeset_id: b.clone(),
                    path_prefix: None,
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(tree.entries.len(), 1);
        assert_eq!(tree.entries[0].path, "dir/f.txt");
        assert_eq!(tree.entries[0].mode, VcsFileMode::Regular);

        let diff = r
            .api
            .diff(
                &r.repo_id,
                VcsDiffRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    from: a,
                    to: b.clone(),
                    paths: None,
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(diff.entries.len(), 1);
        assert_eq!(diff.entries[0].kind, ManifestEntryKind::Modify);
        assert_eq!(diff.totals.files, 1);

        let file = r
            .api
            .file_get(
                &r.repo_id,
                VcsFileGetRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    changeset_id: b.clone(),
                    path: "dir/f.txt".into(),
                },
            )
            .await
            .unwrap();
        assert!(!file.too_large);
        let content = base64::engine::general_purpose::STANDARD
            .decode(file.content_b64.unwrap())
            .unwrap();
        assert_eq!(content, b"line two\n");
        assert_eq!(file.size, 9);
    }

    #[tokio::test]
    async fn file_get_big_blob_is_too_large() {
        let r = rig(false).await;
        enable(&r).await;
        // Seed a >256KiB blob directly through the object store, then a
        // changeset referencing it by hash.
        let big = vec![b'y'; INLINE_CONTENT_MAX + 10];
        let stream = stream::iter(vec![Ok::<Bytes, std::io::Error>(Bytes::from(big.clone()))]);
        let hash = r.api.objects().put(stream, &r.repo_id).await.unwrap().hash;
        let id = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(
                    vec![],
                    "big",
                    vec![ManifestEntry {
                        path: "big.bin".into(),
                        kind: ManifestEntryKind::Add,
                        mode: VcsFileMode::Regular,
                        old_path: None,
                        blob: Some(ObjectId::parse(&format!("sha256:{hash}")).unwrap()),
                        content_b64: None,
                    }],
                ),
            )
            .await
            .unwrap()
            .changeset_id;
        let file = r
            .api
            .file_get(
                &r.repo_id,
                VcsFileGetRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    changeset_id: id,
                    path: "big.bin".into(),
                },
            )
            .await
            .unwrap();
        assert!(file.too_large);
        assert!(file.content_b64.is_none());
    }

    #[tokio::test]
    async fn file_get_missing_path_not_found() {
        let r = rig(false).await;
        enable(&r).await;
        let id = seed_cs(&r, "one", "a", b"1").await;
        let err = r
            .api
            .file_get(
                &r.repo_id,
                VcsFileGetRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    changeset_id: id,
                    path: "nope".into(),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.not_found");
    }

    #[tokio::test]
    async fn blame_default_head_main() {
        let r = rig(false).await;
        enable(&r).await;
        let id = seed_cs(&r, "one", "f.txt", b"line\n").await;
        r.api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Null,
                    new: Nullable::Value(id.clone()),
                },
            )
            .await
            .unwrap();
        let res = r
            .api
            .blame(
                &r.repo_id,
                VcsBlameRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    path: "f.txt".into(),
                    at: None,
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(res.entries.len(), 1);
        assert_eq!(res.entries[0].content, "line");
        assert_eq!(res.entries[0].author.name, "U Api");
    }

    #[tokio::test]
    async fn blame_no_refs_not_found_ambiguous_invalid_input() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "one", "f.txt", b"line\n").await;
        // Zero refs → vcs.not_found.
        let err = r
            .api
            .blame(
                &r.repo_id,
                VcsBlameRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    path: "f.txt".into(),
                    at: None,
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.not_found");
        // ≥2 refs without `main` → vcs.invalid_input.
        for name in ["x", "y"] {
            r.api
                .ref_update(
                    &r.repo_id,
                    "u-api",
                    VcsRefUpdateRequest {
                        owner: "o".into(),
                        name: "n".into(),
                        ref_name: name.into(),
                        expected_old: Nullable::Null,
                        new: Nullable::Value(a.clone()),
                    },
                )
                .await
                .unwrap();
        }
        let err = r
            .api
            .blame(
                &r.repo_id,
                VcsBlameRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    path: "f.txt".into(),
                    at: None,
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.invalid_input");
    }

    #[tokio::test]
    async fn path_history_reports_touches() {
        let r = rig(false).await;
        enable(&r).await;
        let id = seed_cs(&r, "one", "dir/p.rs", b"x\n").await;
        let res = r
            .api
            .path_history(
                &r.repo_id,
                VcsPathHistoryRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    path: "dir/p.rs".into(),
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(res.entries.len(), 1);
        assert_eq!(res.entries[0].changeset_id, id);
        assert_eq!(res.entries[0].kind, ManifestEntryKind::Add);
    }

    // ---------------- merge_base / merge / rebase ----------------

    #[tokio::test]
    async fn merge_base_and_fast_forward_merge() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "base", "a", b"1").await;
        let b = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![a.clone()], "child", vec![entry("b", b"2")]),
            )
            .await
            .unwrap()
            .changeset_id;
        let base = r
            .api
            .merge_base(
                &r.repo_id,
                VcsMergeBaseRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    a: a.clone(),
                    b: b.clone(),
                },
            )
            .await
            .unwrap();
        assert_eq!(base.merge_base, a);
        // ancestor pair → fast-path, no merge node.
        let m = r
            .api
            .merge(
                &r.repo_id,
                "u-api",
                &actor(),
                VcsMergeRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    a: a.clone(),
                    b: b.clone(),
                    message: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(m.changeset_id, b);
        assert_eq!(m.fast_forward, Some(true));
        let (rows, _) =
            r.db.list_repo_activity(&r.repo_id, None, None, 0, 20)
                .await
                .unwrap();
        let row = rows
            .iter()
            .find(|x| x.push_type == VCS_ACTIVITY_MERGE)
            .expect("merge activity row");
        assert_eq!(row.after_oid.as_deref(), Some(b.as_str()));
        assert_eq!(row.commits_count, Some(0));
    }

    #[tokio::test]
    async fn merge_base_disjoint_histories_no_common_ancestor() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "one", "a", b"1").await;
        let b = seed_cs(&r, "two", "b", b"2").await;
        let err = r
            .api
            .merge_base(
                &r.repo_id,
                VcsMergeBaseRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    a,
                    b,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.no_common_ancestor");
    }

    #[tokio::test]
    async fn rebase_replays_onto_new_parent() {
        let r = rig(false).await;
        enable(&r).await;
        let base = seed_cs(&r, "base", "a", b"1").await;
        let other = seed_cs(&r, "other", "o", b"2").await;
        let child = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![base], "child", vec![entry("c", b"3")]),
            )
            .await
            .unwrap()
            .changeset_id;
        let res = r
            .api
            .rebase(
                &r.repo_id,
                "u-api",
                &actor(),
                VcsRebaseRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    changeset_id: child,
                    new_parent_id: other.clone(),
                    message: None,
                },
            )
            .await
            .unwrap();
        let got = r
            .api
            .changeset_get(&r.repo_id, &r.bare, res.head.as_str())
            .await
            .unwrap();
        assert_eq!(got.parents, vec![other]);
        let (rows, _) =
            r.db.list_repo_activity(&r.repo_id, None, None, 0, 20)
                .await
                .unwrap();
        assert!(rows.iter().any(|x| x.push_type == VCS_ACTIVITY_REBASE));
    }

    // ---------------- refs ----------------

    #[tokio::test]
    async fn ref_update_cas_create_move_delete_conflict() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "one", "a", b"1").await;
        let b = r
            .api
            .changeset_create(
                &r.repo_id,
                "u-api",
                &actor(),
                create_req(vec![a.clone()], "two", vec![entry("b", b"2")]),
            )
            .await
            .unwrap()
            .changeset_id;
        // create
        let res = r
            .api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Null,
                    new: Nullable::Value(a.clone()),
                },
            )
            .await
            .unwrap();
        assert_eq!(res.changeset_id, Some(a.clone()));
        // stale expected → ref_conflict carrying the actual head.
        let err = r
            .api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Value(b.clone()),
                    new: Nullable::Value(b.clone()),
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.ref_conflict");
        // move with correct expected
        r.api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Value(a.clone()),
                    new: Nullable::Value(b.clone()),
                },
            )
            .await
            .unwrap();
        // delete → changeset_id null on the response.
        let res = r
            .api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Value(b),
                    new: Nullable::Null,
                },
            )
            .await
            .unwrap();
        assert!(res.changeset_id.is_none());
        // delete on absent → ref_conflict{actual:null}
        let err = r
            .api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Value(a),
                    new: Nullable::Null,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.ref_conflict");
        let (rows, _) =
            r.db.list_repo_activity(&r.repo_id, None, None, 0, 20)
                .await
                .unwrap();
        assert_eq!(
            rows.iter()
                .filter(|x| x.push_type == VCS_ACTIVITY_REF_UPDATE)
                .count(),
            3
        );
    }

    #[tokio::test]
    async fn ref_list_returns_refs() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "one", "a", b"1").await;
        for name in ["main", "dev"] {
            r.api
                .ref_update(
                    &r.repo_id,
                    "u-api",
                    VcsRefUpdateRequest {
                        owner: "o".into(),
                        name: "n".into(),
                        ref_name: name.into(),
                        expected_old: Nullable::Null,
                        new: Nullable::Value(a.clone()),
                    },
                )
                .await
                .unwrap();
        }
        let res = r
            .api
            .ref_list(
                &r.repo_id,
                VcsRefListRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(res.entries.len(), 2);
        assert!(res.entries.iter().all(|e| e.changeset_id == a));
    }

    // ---------------- interop ----------------

    #[tokio::test]
    async fn git_import_then_get_by_oid_and_status() {
        let r = rig(true).await;
        enable(&r).await;
        seed_git(&r, "c1", &[("f.txt", b"git one\n")]).await;
        let page = r
            .api
            .git_import(
                &r.repo_id,
                &r.bare,
                "u-api",
                VcsGitImportRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    from: None,
                    cursor: None,
                    limit: None,
                },
            )
            .await
            .unwrap();
        assert!(page.done);
        assert_eq!(page.imported, 1);
        assert_eq!(page.heads.len(), 1);
        let cs = page.heads[0].changeset_id.clone().expect("head landed");
        let got = r
            .api
            .changeset_get(&r.repo_id, &r.bare, &page.heads[0].git_oid)
            .await
            .unwrap();
        assert_eq!(got.changeset_id, cs);
        let st = r.api.status(&r.repo_id).await.unwrap();
        assert_eq!(st.changeset_count, 1);
        let (rows, _) =
            r.db.list_repo_activity(&r.repo_id, None, None, 0, 20)
                .await
                .unwrap();
        assert!(rows.iter().any(|x| x.push_type == VCS_ACTIVITY_GIT_IMPORT));
    }

    #[tokio::test]
    async fn changeset_get_git_oid_lazy_imports() {
        let r = rig(true).await;
        enable(&r).await;
        seed_git(&r, "lazy", &[("l.txt", b"lazy\n")]).await;
        let oid = {
            let out = tokio::process::Command::new("git")
                .args(["rev-parse", "refs/heads/main"])
                .current_dir(&r.bare)
                .output()
                .await
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        // no import ran — the read triggers lazy_import via resolve_git_oid.
        let got = r
            .api
            .changeset_get(&r.repo_id, &r.bare, &oid)
            .await
            .unwrap();
        assert!(got.changeset_id.as_str().starts_with("cs_"));
    }

    #[tokio::test]
    async fn git_export_round_trip_and_activity() {
        let r = rig(true).await;
        enable(&r).await;
        let id = seed_cs(&r, "out", "x.txt", b"export me\n").await;
        let res = r
            .api
            .git_export(
                &r.repo_id,
                &r.bare,
                "u-api",
                "admin",
                VcsGitExportRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    head: id,
                    branch: "release".into(),
                    force: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(res.branch, "release");
        assert_eq!(res.git_oid.len(), 40);
        let (rows, _) =
            r.db.list_repo_activity(&r.repo_id, None, None, 0, 20)
                .await
                .unwrap();
        let row = rows
            .iter()
            .find(|x| x.push_type == VCS_ACTIVITY_GIT_EXPORT)
            .expect("export activity row");
        assert_eq!(row.ref_name.as_deref(), Some("refs/heads/release"));
        assert_eq!(row.after_oid.as_deref(), Some(res.git_oid.as_str()));
    }

    #[tokio::test]
    async fn git_export_denied_maps_forbidden() {
        let r = rig_with_check(true, Arc::new(Deny)).await;
        enable(&r).await;
        let id = seed_cs(&r, "out", "x.txt", b"x\n").await;
        let err = r
            .api
            .git_export(
                &r.repo_id,
                &r.bare,
                "u-api",
                "write",
                VcsGitExportRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    head: id,
                    branch: "main".into(),
                    force: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.forbidden");
        assert!(err.data.is_some());
    }

    #[tokio::test]
    async fn git_import_without_interop_is_disabled() {
        let r = rig(false).await;
        enable(&r).await;
        let err = r
            .api
            .git_import(
                &r.repo_id,
                &r.bare,
                "u-api",
                VcsGitImportRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    from: None,
                    cursor: None,
                    limit: None,
                },
            )
            .await
            .unwrap_err();
        assert_eq!(err_code(&err), "vcs.disabled");
    }

    #[tokio::test]
    async fn log_git_oid_from_ref_imports_then_walks() {
        let r = rig(true).await;
        enable(&r).await;
        seed_git(&r, "walk", &[("w.txt", b"w\n")]).await;
        let oid = {
            let out = tokio::process::Command::new("git")
                .args(["rev-parse", "refs/heads/main"])
                .current_dir(&r.bare)
                .output()
                .await
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        let res = r
            .api
            .log(
                &r.repo_id,
                &r.bare,
                VcsLogRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    from: oid,
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(res.entries.len(), 1);
        assert_eq!(res.entries[0].message.trim(), "walk");
    }

    #[tokio::test]
    async fn status_reports_heads_and_counts() {
        let r = rig(false).await;
        enable(&r).await;
        let a = seed_cs(&r, "one", "a", b"1").await;
        r.api
            .ref_update(
                &r.repo_id,
                "u-api",
                VcsRefUpdateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    ref_name: "main".into(),
                    expected_old: Nullable::Null,
                    new: Nullable::Value(a.clone()),
                },
            )
            .await
            .unwrap();
        let st = r.api.status(&r.repo_id).await.unwrap();
        assert!(st.enabled);
        assert_eq!(st.heads.len(), 1);
        assert_eq!(st.heads[0].name, "main");
        assert_eq!(st.changeset_count, 1);
        assert_eq!(st.object_count, 1);
        assert!(st.storage_bytes > 0);
    }

    /// `Database::skipped()` ⇒ `db.not_configured` before any store call —
    /// the flag-free admin path, the read gate, and the write gate all fail
    /// closed (skipped-db facades return opaque strings, so each surface
    /// checks `is_skipped` itself).
    #[tokio::test]
    async fn skipped_db_fails_closed() {
        let odir = tempfile::tempdir().unwrap();
        let db = Database::skipped();
        let objects = ObjectStore::new(db.clone(), cfg(odir.path())).unwrap();
        let index = ReadIndex::new(
            db.clone(),
            ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
            ReadIndexCaps::default(),
        );
        let merge = MergeEngine::new(
            db.clone(),
            ReadIndex::new(
                db.clone(),
                ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
                ReadIndexCaps::default(),
            ),
            ChangesetStore::new(db.clone()),
            ObjectStore::new(db.clone(), cfg(odir.path())).unwrap(),
            MergeCaps::default(),
        )
        .unwrap();
        let api = VcsApi::new(db, objects, index, merge, None);

        let author = VcsAuthor {
            name: "t".into(),
            email: "t@t".into(),
        };
        for err in [
            api.repo_enable("r", "u", true).await.unwrap_err(),
            api.status("r").await.unwrap_err(),
            api.ref_list(
                "r",
                VcsRefListRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    limit: None,
                    cursor: None,
                },
            )
            .await
            .unwrap_err(),
            api.changeset_create(
                "r",
                "u",
                &author,
                VcsChangesetCreateRequest {
                    owner: "o".into(),
                    name: "n".into(),
                    parents: vec![],
                    message: "m".into(),
                    author: None,
                    changes: vec![],
                },
            )
            .await
            .unwrap_err(),
        ] {
            assert_eq!(err_code(&err), "db.not_configured", "{err:?}");
        }
    }
}
