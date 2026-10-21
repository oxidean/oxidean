//! Per-repository collaborator CRUD (`repo.collaborators.*` — ORG-03 / D-ORG-02c / D-ORG-04).
//!
//! Admin-gated via Capability ACL (T-10-01 / T-10-02). Collaborator is never an org role.

use oxidean_core::AppError;

use crate::auth::gate::require_verified;
use crate::repo::{acl, meets, resolve_repo_for_read, AccessibleRepo, Capability};
use crate::rpc::RpcCtx;

/// Resolve repo for Admin-only mutate (collaborators, visibility, soft-delete).
/// Missing OR insufficient capability → identical soft [`acl::not_found`].
pub async fn resolve_repo_for_admin(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
) -> Result<AccessibleRepo, AppError> {
    let _ = require_verified(ctx).await?;
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Admin) {
        return Err(acl::not_found());
    }
    Ok(accessible)
}
