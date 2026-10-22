//! `vcs.*` RPC handlers (contract C1) — extracted from `core::vcs` (#162).
//! The `VcsSlot`/`SharedVcs`/`vcs_api` slot stays in core; handlers resolve it
//! through `oxidean_api_core::vcs::vcs_api`.

use oxidean_core::{
    AppError, VcsAuthor, VcsBlameRequest, VcsBlameResponse, VcsChangesetCreateRequest,
    VcsChangesetCreateResponse, VcsChangesetGetRequest, VcsChangesetPublic, VcsDiffRequest,
    VcsDiffResponse, VcsErrorCode, VcsFileGetRequest, VcsFileGetResponse, VcsGitExportRequest,
    VcsGitExportResponse, VcsGitImportRequest, VcsGitImportResponse, VcsLogRequest, VcsLogResponse,
    VcsMergeBaseRequest, VcsMergeBaseResponse, VcsMergeRequest, VcsMergeResponse,
    VcsPathHistoryRequest, VcsPathHistoryResponse, VcsRebaseRequest, VcsRebaseResponse,
    VcsRefListRequest, VcsRefListResponse, VcsRefUpdateRequest, VcsRefUpdateResponse,
    VcsRepoEnableRequest, VcsRepoEnableResponse, VcsStatusRequest, VcsStatusResponse,
    VcsTreeRequest, VcsTreeResponse,
};
use oxidean_db::UserRow;

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::repo::{
    resolve_repo_for_admin, resolve_repo_for_owner_mutate, resolve_repo_for_read, AccessibleRepo,
};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_api_core::vcs::vcs_api;

fn bad_input(proc: &str, e: serde_json::Error) -> AppError {
    AppError::new("rpc.bad_input", format!("invalid {proc} input: {e}"))
}

/// `auth.unauthenticated` from the shared resolver remaps onto the C1
/// taxonomy; everything else (`auth.email_unverified`, `repo.*`, …) passes
/// through unchanged so the wire keeps the real reason.
fn map_auth(e: AppError) -> AppError {
    match e.code.as_str() {
        "auth.unauthenticated" => {
            VcsErrorCode::Unauthenticated.app_error("authentication required")
        }
        _ => e,
    }
}

fn actor(user: &UserRow) -> VcsAuthor {
    let name = if user.display_name.trim().is_empty() {
        user.username.clone()
    } else {
        user.display_name.clone()
    };
    VcsAuthor {
        name,
        email: user.email.clone(),
    }
}

/// `vcs.repo_enable` — Admin tier (BR1.3).
pub async fn repo_enable(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsRepoEnableResponse, AppError> {
    let req: VcsRepoEnableRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.repo_enable", e))?;
    let user = require_verified(ctx).await.map_err(map_auth)?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?
        .repo_enable(&accessible.row.id, &user.id, req.enabled)
        .await
}

/// `vcs.status` — Read tier; flag-free.
pub async fn status(ctx: &RpcCtx, input: serde_json::Value) -> Result<VcsStatusResponse, AppError> {
    let req: VcsStatusRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.status", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.status(&accessible.row.id).await
}

pub async fn changeset_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsChangesetCreateResponse, AppError> {
    let req: VcsChangesetCreateRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.changeset_create", e))?;
    let (user, accessible) = write_access(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?
        .changeset_create(&accessible.row.id, &user.id, &actor(&user), req)
        .await
}

pub async fn changeset_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsChangesetPublic, AppError> {
    let req: VcsChangesetGetRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.changeset_get", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let repo = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    vcs_api(ctx)?
        .changeset_get(&accessible.row.id, &repo, &req.id)
        .await
}

pub async fn log(ctx: &RpcCtx, input: serde_json::Value) -> Result<VcsLogResponse, AppError> {
    let req: VcsLogRequest = serde_json::from_value(input).map_err(|e| bad_input("vcs.log", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let repo = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    vcs_api(ctx)?.log(&accessible.row.id, &repo, req).await
}

pub async fn diff(ctx: &RpcCtx, input: serde_json::Value) -> Result<VcsDiffResponse, AppError> {
    let req: VcsDiffRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.diff", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.diff(&accessible.row.id, req).await
}

pub async fn tree(ctx: &RpcCtx, input: serde_json::Value) -> Result<VcsTreeResponse, AppError> {
    let req: VcsTreeRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.tree", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.tree(&accessible.row.id, req).await
}

pub async fn file_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsFileGetResponse, AppError> {
    let req: VcsFileGetRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.file_get", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.file_get(&accessible.row.id, req).await
}

pub async fn blame(ctx: &RpcCtx, input: serde_json::Value) -> Result<VcsBlameResponse, AppError> {
    let req: VcsBlameRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.blame", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.blame(&accessible.row.id, req).await
}

pub async fn path_history(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsPathHistoryResponse, AppError> {
    let req: VcsPathHistoryRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.path_history", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.path_history(&accessible.row.id, req).await
}

pub async fn merge_base(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsMergeBaseResponse, AppError> {
    let req: VcsMergeBaseRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.merge_base", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.merge_base(&accessible.row.id, req).await
}

pub async fn merge(ctx: &RpcCtx, input: serde_json::Value) -> Result<VcsMergeResponse, AppError> {
    let req: VcsMergeRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.merge", e))?;
    let (user, accessible) = write_access(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?
        .merge(&accessible.row.id, &user.id, &actor(&user), req)
        .await
}

pub async fn rebase(ctx: &RpcCtx, input: serde_json::Value) -> Result<VcsRebaseResponse, AppError> {
    let req: VcsRebaseRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.rebase", e))?;
    let (user, accessible) = write_access(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?
        .rebase(&accessible.row.id, &user.id, &actor(&user), req)
        .await
}

pub async fn ref_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsRefListResponse, AppError> {
    let req: VcsRefListRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.ref_list", e))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?.ref_list(&accessible.row.id, req).await
}

pub async fn ref_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsRefUpdateResponse, AppError> {
    let req: VcsRefUpdateRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.ref_update", e))?;
    let (user, accessible) = write_access(ctx, &req.owner, &req.name).await?;
    vcs_api(ctx)?
        .ref_update(&accessible.row.id, &user.id, req)
        .await
}

pub async fn git_import(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsGitImportResponse, AppError> {
    let req: VcsGitImportRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.git_import", e))?;
    let (user, accessible) = write_access(ctx, &req.owner, &req.name).await?;
    let repo = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    vcs_api(ctx)?
        .git_import(&accessible.row.id, &repo, &user.id, req)
        .await
}

pub async fn git_export(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<VcsGitExportResponse, AppError> {
    let req: VcsGitExportRequest =
        serde_json::from_value(input).map_err(|e| bad_input("vcs.git_export", e))?;
    let (user, accessible) = write_access(ctx, &req.owner, &req.name).await?;
    let repo = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let capability = oxidean_api_core::repo::capability_env_label(accessible.capability);
    vcs_api(ctx)?
        .git_export(&accessible.row.id, &repo, &user.id, capability, req)
        .await
}

/// Verified session + `Write` capability + not-archived — the pinned write
/// tier (matches `resolve_repo_for_owner_mutate`; `auth.unauthenticated`
/// remaps to `vcs.unauthenticated` per BR1.6).
async fn write_access(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<(UserRow, AccessibleRepo), AppError> {
    let user = require_verified(ctx).await.map_err(map_auth)?;
    let accessible = resolve_repo_for_owner_mutate(ctx, owner, name).await?;
    Ok((user, accessible))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_unmap_passes_through_non_auth() {
        let e = map_auth(AppError::new("repo.not_found", "nope"));
        assert_eq!(e.code, "repo.not_found");
        let e = map_auth(AppError::new("auth.unauthenticated", "not authenticated"));
        assert_eq!(e.code, "vcs.unauthenticated");
    }
}

#[test]
fn auth_unmap_passes_through_non_auth() {
    let e = map_auth(AppError::new("repo.not_found", "nope"));
    assert_eq!(e.code, "repo.not_found");
    let e = map_auth(AppError::new("auth.unauthenticated", "not authenticated"));
    assert_eq!(e.code, "vcs.unauthenticated");
}
