//! Pull ACL — reuse repo Capability ladder (D-PR-29).

use crate::repo::units::{require_unit_enabled, RepoUnit};
use crate::repo::{
    meets, not_found, resolve_repo_for_admin, resolve_repo_for_read, AccessibleRepo, Capability,
};
use crate::rpc::RpcCtx;
use oxidean_core::AppError;

/// Read+ on the pulls unit surface. Missing / unauthorized private → soft
/// `repo.not_found`; pulls unit disabled → `repo.pulls.disabled` (COL-13).
pub async fn resolve_for_read(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    require_unit_enabled(ctx, RepoUnit::Pulls, &accessible.row.id).await?;
    Ok(accessible)
}

/// Write+ on the pulls unit surface; disabled → `repo.pulls.disabled`.
/// GIT-20: archived repositories reject every pull write (read-only).
pub async fn resolve_for_write(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    require_unit_enabled(ctx, RepoUnit::Pulls, &accessible.row.id).await?;
    crate::repo::ensure_not_archived(&accessible)?;
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

/// Admin resolve for **repo settings** surfaces (COL-13): intentionally not
/// gated on the pulls flag — `repo.mergeSettings.update` stays usable so an
/// admin can pre-configure merge options before enabling the unit.
pub async fn resolve_for_admin(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    resolve_repo_for_admin(ctx, owner, name).await
}
