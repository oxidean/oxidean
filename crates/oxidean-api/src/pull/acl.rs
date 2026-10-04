//! Pull ACL — reuse repo Capability ladder (D-PR-29).

use crate::repo::{
    meets, not_found, resolve_repo_for_admin, resolve_repo_for_read, AccessibleRepo, Capability,
};
use crate::rpc::RpcCtx;
use oxidean_core::AppError;

pub async fn resolve_for_read(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    resolve_repo_for_read(ctx, owner, name).await
}

pub async fn resolve_for_write(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    Ok(accessible)
}

/// PR author **or** Write+ may edit/close/reopen the pull (author participation).
pub fn can_edit_pull(
    user_id: &str,
    pull: &oxidean_db::PullRow,
    capability: Option<Capability>,
) -> bool {
    pull.author_id == user_id || meets(capability, Capability::Write)
}

#[allow(dead_code)]
pub async fn resolve_for_admin(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    resolve_repo_for_admin(ctx, owner, name).await
}
