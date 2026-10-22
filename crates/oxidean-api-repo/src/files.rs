//! `repo.file.*` mutation handlers (issue #162). The commit machinery
//! (`commit_changes`, path validation, base-branch resolution) lives in
//! `oxidean_api_core::repo::files`.

use oxidean_api_core::auth::gate::require_verified;
use oxidean_core::{
    AppError, RepoFileCommitPolicyRequest, RepoFileCommitPolicyResponse, RepoFileCommitResponse,
    RepoFileDeleteRequest, RepoFileMkdirRequest, RepoFileRenameRequest, RepoFileUpdateRequest,
    RepoFileUploadRequest,
};
use std::collections::BTreeSet;

use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::repo::files::{
    assert_ancestors_clear, assert_path_creatable, bad_input, base64_decode, commit_changes,
    commit_message, decode_content, direct_push_allowed, entry_at, invalid_input,
    normalize_edit_path, path_conflict, path_not_found, require_file_entry, resolve_base_branch,
    validate_branch_name, FILE_EDIT_MAX_BYTES, UPLOAD_MAX_FILES, UPLOAD_MAX_TOTAL_BYTES,
};
use oxidean_api_core::repo::resolve_repo_for_owner_mutate;
use oxidean_api_core::rpc::RpcCtx;
use oxidean_git::{FileChange, TreeEntryKind};

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
