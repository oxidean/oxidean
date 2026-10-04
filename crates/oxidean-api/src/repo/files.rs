//! `repo.file.*` — browser file edits committed via the web-flow key (GIT-19).
//!
//! Every mutation lands as a single commit authored by the caller (committer:
//! the forge web-flow identity) and SSH-signed with the instance web-flow key.
//! When the target branch's protection rules reject direct pushes, the change
//! goes to a new `web-edit/*` branch and a pull request is opened instead —
//! same semantics as a git push to a protected branch.

use std::collections::BTreeSet;
use std::path::{Component, Path};

use oxidean_core::{
    AppError, RepoFileCommitOptions, RepoFileCommitPolicyRequest, RepoFileCommitPolicyResponse,
    RepoFileCommitResponse, RepoFileCreateRequest, RepoFileDeleteRequest, RepoFileMkdirRequest,
    RepoFileRenameRequest, RepoFileUpdateRequest, RepoFileUploadRequest,
};
use oxidean_db::UserRow;
use oxidean_git::{FileChange, GitError, TreeEntry, TreeEntryKind};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::rpc::RpcCtx;

use super::{resolve_repo_for_owner_mutate, AccessibleRepo};

/// Decoded per-file byte cap for web edits/uploads (GIT-19). Generous for
/// source files; larger blobs belong to git push / LFS.
pub const FILE_EDIT_MAX_BYTES: usize = 8 * 1024 * 1024;
/// Max files in one `repo.file.upload` commit.
pub const UPLOAD_MAX_FILES: usize = 50;
/// Total decoded bytes per `repo.file.upload`.
pub const UPLOAD_MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
/// Commit message cap (subject + body).
const MESSAGE_MAX_CHARS: usize = 10_000;
/// Git zero OID used as `before` for ref creations.
const ZERO_OID: &str = "0000000000000000000000000000000000000000";

fn invalid_input(msg: impl Into<String>) -> AppError {
    AppError::new("repo.invalid_input", msg.into())
}

fn path_conflict(msg: impl Into<String>) -> AppError {
    AppError::new("repo.path_conflict", msg.into())
}

fn path_not_found(msg: impl Into<String>) -> AppError {
    AppError::new("repo.path_not_found", msg.into())
}

/// Map backend failures to stable API codes (GIT-19). Path/ref validation
/// duplicates live in the backend too — by the time git runs, arguments were
/// already validated here, so `InvalidArg` means a request-shape bug or a
/// client bypassing the UI.
fn file_git_err(e: GitError) -> AppError {
    match e {
        GitError::NotFound(m) => AppError::new("repo.path_not_found", m),
        GitError::InvalidArg(m) if m.contains("no changes") => {
            AppError::new("repo.no_changes", "no changes to commit")
        }
        GitError::InvalidArg(m) => AppError::new("repo.invalid_input", m),
        GitError::Conflict(m) if m.contains("branch already exists") => {
            AppError::new("repo.branch_exists", m)
        }
        GitError::Conflict(m) => AppError::new("repo.conflict", m),
        GitError::Denied(m) => AppError::new(
            "repo.branch_protection",
            "branch protection rules block this update",
        )
        .with_data(serde_json::json!({ "detail": m })),
        other => {
            tracing::error!(error = %other, "git backend error during file commit");
            AppError::new("repo.git_failed", "git operation failed")
        }
    }
}

/// Repo-relative path for writes: traversal/NUL/absolute rejection plus no
/// empty segments (`a//b`, trailing `/`), `.` segments, or `.git` components
/// (reserved by git checkout). Mirrors the backend `validate_edit_path` so bad
/// input fails with API error codes before any git work.
fn normalize_edit_path(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.starts_with('/') {
        return Err(invalid_input(format!(
            "absolute paths are not allowed: {raw}"
        )));
    }
    let rel = trimmed;
    if rel.contains('\0') {
        return Err(invalid_input("path contains NUL"));
    }
    if rel.is_empty() {
        return Err(invalid_input("path is required"));
    }
    let candidate = Path::new(rel);
    if candidate.is_absolute()
        || candidate.components().any(|c| {
            matches!(
                c,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(invalid_input(format!("path escapes repo: {raw}")));
    }
    if rel
        .split('/')
        .any(|seg| seg.is_empty() || seg == "." || seg == ".git")
    {
        return Err(invalid_input(format!("invalid repo path: {raw}")));
    }
    Ok(rel.to_string())
}

/// `git check-ref-format`-flavored validation for branch fields (subset —
/// rejects control/whitespace, ref-format metacharacters, `..`, leading `-`,
/// `.lock` suffix, dot-leading components, `@`).
fn validate_branch_name(raw: &str, field: &str) -> Result<(), AppError> {
    let b = raw.trim();
    let bad = b.is_empty()
        || b.len() > 255
        || b.starts_with('-')
        || b == "@"
        || b.contains("..")
        || b.contains("//")
        || b.starts_with('/')
        || b.ends_with('/')
        || b.ends_with('.')
        || b.contains("@{")
        || b.ends_with(".lock")
        || b.split('/')
            .any(|seg| seg.is_empty() || seg.starts_with('.'))
        || b.chars().any(|c| {
            c.is_control()
                || c.is_whitespace()
                || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        });
    if bad {
        return Err(AppError::new(
            "repo.invalid_ref",
            format!("invalid {field}: {raw}"),
        ));
    }
    Ok(())
}

/// Commit message: trimmed, NUL-free, length-capped; blank → per-op default.
fn commit_message(raw: &str, default: &str) -> Result<String, AppError> {
    if raw.contains('\0') {
        return Err(invalid_input("commit message contains NUL"));
    }
    let m = raw.trim();
    let out = if m.is_empty() { default } else { m };
    if out.chars().count() > MESSAGE_MAX_CHARS {
        return Err(invalid_input("commit message is too long"));
    }
    Ok(out.to_string())
}

/// Resolve the base branch (`target.branch` or the repo default) and validate
/// every branch field in the request.
fn resolve_base_branch(
    accessible: &AccessibleRepo,
    target: &RepoFileCommitOptions,
) -> Result<String, AppError> {
    let base = target
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(accessible.row.default_branch.as_str());
    validate_branch_name(base, "branch")?;
    if let Some(nb) = target
        .new_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        validate_branch_name(nb, "new_branch")?;
        if nb == base {
            return Err(invalid_input("new_branch must differ from branch"));
        }
    }
    Ok(base.to_string())
}

/// ls-tree entry for `path` itself on `branch` — blob/tree/gitlink, or None
/// (missing path or unborn branch).
async fn entry_at(
    ctx: &RpcCtx,
    bare: &Path,
    branch: &str,
    path: &str,
) -> Result<Option<TreeEntry>, AppError> {
    ctx.git
        .ls_tree_entry(bare, &format!("refs/heads/{branch}"), path)
        .await
        .map_err(file_git_err)
}

/// Every ancestor segment of `path` must be a directory or absent — a
/// file/gitlink ancestor makes the destination impossible (D/F conflict).
async fn assert_ancestors_clear(
    ctx: &RpcCtx,
    bare: &Path,
    branch: &str,
    path: &str,
) -> Result<(), AppError> {
    let segments: Vec<&str> = path.split('/').collect();
    let mut prefix = String::new();
    for seg in segments.iter().take(segments.len().saturating_sub(1)) {
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(seg);
        match entry_at(ctx, bare, branch, &prefix).await? {
            Some(e) if e.kind != TreeEntryKind::Tree => {
                return Err(path_conflict(format!(
                    "{prefix} is a file — cannot create {path} inside it"
                )));
            }
            Some(_) => {}
            None => break, // absent ancestor → deeper entries cannot exist
        }
    }
    Ok(())
}

/// `path` must be completely unused on `branch`.
async fn assert_path_creatable(
    ctx: &RpcCtx,
    bare: &Path,
    branch: &str,
    path: &str,
) -> Result<(), AppError> {
    if let Some(e) = entry_at(ctx, bare, branch, path).await? {
        let what = if e.kind == TreeEntryKind::Tree {
            "a directory"
        } else {
            "a file"
        };
        return Err(path_conflict(format!("{path} already exists as {what}")));
    }
    assert_ancestors_clear(ctx, bare, branch, path).await
}

/// `path` must be an existing blob on `branch` — returns its oid.
async fn require_file_entry(
    ctx: &RpcCtx,
    bare: &Path,
    branch: &str,
    path: &str,
) -> Result<TreeEntry, AppError> {
    let entry = entry_at(ctx, bare, branch, path)
        .await?
        .ok_or_else(|| path_not_found(format!("{path} does not exist")))?;
    if entry.kind != TreeEntryKind::Blob {
        return Err(invalid_input(format!("{path} is not a file")));
    }
    Ok(entry)
}

/// Decode the mutually-exclusive content fields. `None`/`None` → empty file
/// (creating a file with no body is a legitimate empty-file commit).
fn decode_content(
    content: &Option<String>,
    content_base64: &Option<String>,
) -> Result<Vec<u8>, AppError> {
    match (content, content_base64) {
        (Some(_), Some(_)) => Err(invalid_input(
            "provide `content` or `content_base64`, not both",
        )),
        (Some(text), None) => {
            if text.contains('\0') {
                // NUL in a JSON string is a binary payload pretending to be
                // text — the editor path stays text-only by design.
                Err(AppError::new(
                    "repo.binary_content",
                    "text content contains NUL bytes — send binary files via content_base64",
                ))
            } else if text.len() > FILE_EDIT_MAX_BYTES {
                Err(AppError::new(
                    "repo.file_too_large",
                    format!("file exceeds {FILE_EDIT_MAX_BYTES} byte limit"),
                ))
            } else {
                Ok(text.clone().into_bytes())
            }
        }
        (None, Some(b64)) => {
            let bytes = base64_decode(b64)
                .ok_or_else(|| invalid_input("content_base64 is not valid base64"))?;
            if bytes.len() > FILE_EDIT_MAX_BYTES {
                return Err(AppError::new(
                    "repo.file_too_large",
                    format!("file exceeds {FILE_EDIT_MAX_BYTES} byte limit"),
                ));
            }
            Ok(bytes)
        }
        (None, None) => Ok(Vec::new()),
    }
}

/// Manual base64 decoder — same shape as `git::web_flow::base64_decode` (no
/// new crates.io dep). Whitespace tolerated; padding terminated.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let bytes: Vec<u8> = input
        .bytes()
        .filter(|&b| !b.is_ascii_whitespace())
        .collect();
    if bytes.is_empty() {
        // Empty payload = empty file (web uploads may be zero-length).
        return Some(Vec::new());
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for &b in &bytes {
        if b == b'=' {
            break;
        }
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Whether the caller may push straight onto `branch` (protection evaluator —
/// mirrors what `hooks/update` would do to a receive-pack).
async fn direct_push_allowed(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    branch: &str,
) -> Result<bool, AppError> {
    let eff = crate::protection::effective_for_branch(&ctx.db, &accessible.row.id, branch).await?;
    Ok(crate::protection::evaluate_push(
        &eff,
        crate::protection::ProtectionIntent::Push,
        accessible.capability,
    )
    .is_ok())
}

/// Generated fallback branch name for protected-base commits.
fn generate_web_edit_branch(username: &str) -> String {
    let slug: String = username
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let rand = Uuid::new_v4().simple().to_string();
    format!("web-edit/{slug}-{}", &rand[..8])
}

/// Shared tail of every `repo.file.*` mutation: pick the commit target
/// (direct vs new-branch + PR), run the signed commit, then fan out activity,
/// webhooks, PR sync, Actions and mirrors exactly like a receive-pack push
/// (GIT-19).
async fn commit_changes(
    ctx: &RpcCtx,
    accessible: &AccessibleRepo,
    user: &UserRow,
    base_branch: &str,
    target: &RepoFileCommitOptions,
    message: &str,
    changes: Vec<FileChange>,
) -> Result<RepoFileCommitResponse, AppError> {
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;

    let mut new_branch = target
        .new_branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let mut open_pr = target.open_pr.unwrap_or_else(|| new_branch.is_some());
    if !direct_push_allowed(ctx, accessible, base_branch).await? {
        // GIT-19 protected flow: never write to the protected base directly.
        if new_branch.is_none() {
            new_branch = Some(generate_web_edit_branch(&user.username));
        }
        open_pr = true;
    }
    let created_branch = new_branch.is_some();
    let commit_branch = new_branch.unwrap_or_else(|| base_branch.to_string());

    if created_branch {
        // Early, friendly check — commit_files re-verifies against races.
        if ctx
            .git
            .rev_parse(&bare, &format!("refs/heads/{commit_branch}"))
            .await
            .is_ok()
        {
            return Err(AppError::new(
                "repo.branch_exists",
                format!("branch already exists: {commit_branch}"),
            ));
        }
    }

    let signing_key = crate::git::web_flow::ensure_web_flow_key()
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "web-flow signing key unavailable for file commit");
            AppError::new(
                "repo.git_signing_unavailable",
                "web-flow signing key is unavailable",
            )
        })?;
    let author_name = if user.display_name.trim().is_empty() {
        user.username.clone()
    } else {
        user.display_name.clone()
    };

    let result = ctx
        .git
        .commit_files(
            &bare,
            &commit_branch,
            base_branch,
            message,
            &changes,
            &author_name,
            &user.email,
            Some(signing_key.as_path()),
            // The update hook sees the actor's real capability — a rule
            // created between our evaluate_push check and the push still
            // bites (same contract as SSH/Smart-HTTP receive-pack).
            crate::ssh::pack::capability_env_label(accessible.capability),
        )
        .await
        .map_err(file_git_err)?;

    // Post-push fan-out identical to a receive-pack (D-HOOK-10 / D-ACT-05 /
    // mirror sync / PR head refresh).
    let ref_name = format!("refs/heads/{commit_branch}");
    // A created branch is a ref *creation* (before = 0…); an in-place commit
    // is a ref update whose before is the base tip the commit was built on.
    let before = if created_branch {
        ZERO_OID.to_string()
    } else {
        result
            .base_sha
            .clone()
            .unwrap_or_else(|| ZERO_OID.to_string())
    };
    let updates = vec![(before, result.sha.clone(), ref_name)];
    super::record_ref_updates(
        &ctx.db,
        &accessible.row.id,
        &user.id,
        &updates,
        Some(ctx.git.clone()),
        Some(bare.as_path()),
    )
    .await;
    crate::webhook::dispatch::notify_push(
        &ctx.db,
        &accessible.row.id,
        &accessible.owner_username,
        &accessible.row.name,
        &user.username,
        &user.id,
        &updates,
        &ctx.env_name,
    )
    .await;
    crate::pull::synchronize_after_push(
        &ctx.db,
        &ctx.repos_dir,
        &accessible.row.id,
        &accessible.owner_username,
        &accessible.row.name,
        &user.username,
        &user.id,
        &updates,
        &ctx.env_name,
    )
    .await;
    crate::actions::notify_push_actions(
        &ctx.db,
        ctx.git.clone(),
        &ctx.repos_dir,
        &accessible.row.id,
        &accessible.owner_username,
        &accessible.row.name,
        Some(&user.id),
        &updates,
        crate::actions::env_actions_enabled(),
    )
    .await;
    crate::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        accessible.row.id.clone(),
    );

    // Open the PR when the flow created (or the caller supplied) a new branch.
    let mut pr_number = None;
    if open_pr && created_branch {
        let title = target
            .pr_title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                message
                    .lines()
                    .next()
                    .unwrap_or(message)
                    .chars()
                    .take(256)
                    .collect()
            });
        let body = target.pr_body.clone().unwrap_or_default();
        let pr = crate::pull::create(
            ctx,
            serde_json::json!({
                "owner": accessible.owner_username,
                "name": accessible.row.name,
                "title": title,
                "body": body,
                "base_ref": base_branch,
                "head_ref": commit_branch,
            }),
        )
        .await?;
        pr_number = Some(pr.number);
    }

    Ok(RepoFileCommitResponse {
        commit_sha: result.sha,
        branch: commit_branch,
        created_branch,
        pr_number,
    })
}

fn bad_input(proc: &str, e: serde_json::Error) -> AppError {
    AppError::new("rpc.bad_input", format!("invalid {proc} input: {e}"))
}

/// `repo.file.create` — Write+; `path` must be free on the base branch.
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: RepoFileCreateRequest =
        serde_json::from_value(input).map_err(|e| bad_input("repo.file.create", e))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let base_branch = resolve_base_branch(&accessible, &req.target)?;
    let path = normalize_edit_path(&req.path)?;
    let content = decode_content(&req.content, &req.content_base64)?;
    let message = commit_message(&req.message, &format!("Create {path}"))?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    assert_path_creatable(ctx, &bare, &base_branch, &path).await?;
    commit_changes(
        ctx,
        &accessible,
        &user,
        &base_branch,
        &req.target,
        &message,
        vec![FileChange::Upsert { path, content }],
    )
    .await
}

/// `repo.file.update` — Write+; `path` must be an existing file.
pub async fn update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: RepoFileUpdateRequest =
        serde_json::from_value(input).map_err(|e| bad_input("repo.file.update", e))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let base_branch = resolve_base_branch(&accessible, &req.target)?;
    let path = normalize_edit_path(&req.path)?;
    let content = decode_content(&req.content, &req.content_base64)?;
    let message = commit_message(&req.message, &format!("Update {path}"))?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    require_file_entry(ctx, &bare, &base_branch, &path).await?;
    commit_changes(
        ctx,
        &accessible,
        &user,
        &base_branch,
        &req.target,
        &message,
        vec![FileChange::Upsert { path, content }],
    )
    .await
}

/// `repo.file.delete` — Write+; file or whole directory.
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: RepoFileDeleteRequest =
        serde_json::from_value(input).map_err(|e| bad_input("repo.file.delete", e))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let base_branch = resolve_base_branch(&accessible, &req.target)?;
    let path = normalize_edit_path(&req.path)?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let entry = entry_at(ctx, &bare, &base_branch, &path)
        .await?
        .ok_or_else(|| path_not_found(format!("{path} does not exist")))?;
    let default_msg = if entry.kind == TreeEntryKind::Tree {
        format!("Delete directory {path}")
    } else {
        format!("Delete {path}")
    };
    let message = commit_message(&req.message, &default_msg)?;
    commit_changes(
        ctx,
        &accessible,
        &user,
        &base_branch,
        &req.target,
        &message,
        vec![FileChange::Delete { path }],
    )
    .await
}

/// `repo.file.rename` — Write+; file-only (directories are renamed by
/// renaming each child). Optional content replaces bytes in the same commit.
pub async fn rename(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: RepoFileRenameRequest =
        serde_json::from_value(input).map_err(|e| bad_input("repo.file.rename", e))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let base_branch = resolve_base_branch(&accessible, &req.target)?;
    let from = normalize_edit_path(&req.from_path)?;
    let to = normalize_edit_path(&req.to_path)?;
    if from == to {
        return Err(invalid_input("from_path and to_path are identical"));
    }
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let entry = require_file_entry(ctx, &bare, &base_branch, &from).await?;
    assert_path_creatable(ctx, &bare, &base_branch, &to).await?;

    let (changes, default_msg) = if req.content.is_some() || req.content_base64.is_some() {
        let bytes = decode_content(&req.content, &req.content_base64)?;
        (
            vec![
                FileChange::Delete { path: from.clone() },
                FileChange::Upsert {
                    path: to.clone(),
                    content: bytes,
                },
            ],
            format!("Rename {from} to {to}"),
        )
    } else {
        (
            vec![
                FileChange::Delete { path: from.clone() },
                FileChange::UpsertOid {
                    path: to.clone(),
                    oid: entry.oid,
                },
            ],
            format!("Rename {from} to {to}"),
        )
    };
    let message = commit_message(&req.message, &default_msg)?;
    commit_changes(
        ctx,
        &accessible,
        &user,
        &base_branch,
        &req.target,
        &message,
        changes,
    )
    .await
}

/// `repo.file.upload` — Write+; several files in one atomic commit. Existing
/// files are overwritten; existing directories conflict. Binary-safe via
/// base64 (the only path that can carry non-UTF-8 bytes).
pub async fn upload(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: RepoFileUploadRequest =
        serde_json::from_value(input).map_err(|e| bad_input("repo.file.upload", e))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let base_branch = resolve_base_branch(&accessible, &req.target)?;
    let message = commit_message(&req.message, "Add files via upload")?;
    if req.files.is_empty() {
        return Err(invalid_input("at least one file is required"));
    }
    if req.files.len() > UPLOAD_MAX_FILES {
        return Err(AppError::new(
            "repo.too_many_files",
            format!("upload is limited to {UPLOAD_MAX_FILES} files per commit"),
        ));
    }
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let mut seen = BTreeSet::new();
    let mut total: usize = 0;
    let mut changes = Vec::with_capacity(req.files.len());
    for file in &req.files {
        let path = normalize_edit_path(&file.path)?;
        if !seen.insert(path.clone()) {
            return Err(invalid_input(format!("duplicate path in upload: {path}")));
        }
        let bytes = base64_decode(&file.content_base64)
            .ok_or_else(|| invalid_input(format!("{path}: content_base64 is not valid base64")))?;
        if bytes.len() > FILE_EDIT_MAX_BYTES {
            return Err(AppError::new(
                "repo.file_too_large",
                format!("{path} exceeds {FILE_EDIT_MAX_BYTES} byte limit"),
            ));
        }
        total += bytes.len();
        if total > UPLOAD_MAX_TOTAL_BYTES {
            return Err(AppError::new(
                "repo.file_too_large",
                format!("upload exceeds {UPLOAD_MAX_TOTAL_BYTES} byte total limit"),
            ));
        }
        if let Some(e) = entry_at(ctx, &bare, &base_branch, &path).await? {
            if e.kind == TreeEntryKind::Tree {
                return Err(path_conflict(format!("{path} is an existing directory")));
            }
        }
        assert_ancestors_clear(ctx, &bare, &base_branch, &path).await?;
        changes.push(FileChange::Upsert {
            path,
            content: bytes,
        });
    }
    commit_changes(
        ctx,
        &accessible,
        &user,
        &base_branch,
        &req.target,
        &message,
        changes,
    )
    .await
}

/// `repo.file.mkdir` — Write+. Git has no empty-directory object, so the
/// commit materializes `{path}/.gitkeep` (empty file) — the same convention
/// users reach for manually. `path` must not already exist.
pub async fn mkdir(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: RepoFileMkdirRequest =
        serde_json::from_value(input).map_err(|e| bad_input("repo.file.mkdir", e))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let base_branch = resolve_base_branch(&accessible, &req.target)?;
    let dir = normalize_edit_path(&req.path)?;
    let message = commit_message(&req.message, &format!("Create directory {dir}"))?;
    let bare = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    assert_path_creatable(ctx, &bare, &base_branch, &dir).await?;
    commit_changes(
        ctx,
        &accessible,
        &user,
        &base_branch,
        &req.target,
        &message,
        vec![FileChange::Upsert {
            path: format!("{dir}/.gitkeep"),
            content: Vec::new(),
        }],
    )
    .await
}

/// `repo.file.commitPolicy` — Write+; tells the commit form whether the base
/// branch accepts direct commits or needs the new-branch + PR flow.
pub async fn commit_policy(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoFileCommitPolicyResponse, AppError> {
    let req: RepoFileCommitPolicyRequest =
        serde_json::from_value(input).map_err(|e| bad_input("repo.file.commitPolicy", e))?;
    let accessible = resolve_repo_for_owner_mutate(ctx, &req.owner, &req.name).await?;
    let base = req
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(accessible.row.default_branch.as_str());
    validate_branch_name(base, "branch")?;
    let direct = direct_push_allowed(ctx, &accessible, base).await?;
    Ok(RepoFileCommitPolicyResponse {
        branch: base.to_string(),
        direct_commit_allowed: direct,
        requires_pr: !direct,
    })
}
