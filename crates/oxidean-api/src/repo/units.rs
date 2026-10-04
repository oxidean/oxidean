//! Per-repo unit toggles (COL-13): `issues_enabled` / `pulls_enabled`.
//!
//! Each unit ships `repo.<unit>.getEnabled` (Read+) and `repo.<unit>.setEnabled`
//! (Admin) modeled on `repo.actions.*Enabled` (D-ACT-06). The companion
//! `require_<unit>_enabled` gates run inside the issue / pull acl resolvers so
//! every `issue.*` / `pull.*` RPC — including comment, review, label-linking,
//! and merge surfaces — rejects with a stable `repo.<unit>.disabled` code while
//! the unit is off. Disabling never deletes data; the flags are re-checked on
//! every call so re-enabling restores the unit immediately.
//!
//! Git transport, forks, clones, and repository settings (`repo.*`) are not
//! unit surfaces and stay unaffected.

use oxidean_core::{
    AppError, RepoUnitEnabledResponse, RepoUnitGetEnabledRequest, RepoUnitSetEnabledRequest,
};

use crate::auth::gate::require_verified;
use crate::rpc::RpcCtx;

use super::acl::{meets, not_found, resolve_repo_for_read, Capability};
use super::collaborators::resolve_repo_for_admin;
use super::db_err;

/// Which per-repo unit a toggle/gate addresses (COL-13).
#[derive(Clone, Copy)]
pub(crate) enum RepoUnit {
    Issues,
    Pulls,
}

impl RepoUnit {
    fn rpc_namespace(self) -> &'static str {
        match self {
            RepoUnit::Issues => "repo.issues",
            RepoUnit::Pulls => "repo.pulls",
        }
    }

    fn disabled_error(self) -> AppError {
        match self {
            RepoUnit::Issues => AppError::new(
                "repo.issues.disabled",
                "Issues are disabled for this repository",
            ),
            RepoUnit::Pulls => AppError::new(
                "repo.pulls.disabled",
                "Pull requests are disabled for this repository",
            ),
        }
    }
}

/// Gate shared by the `issue.*` / `pull.*` acl resolvers — a disabled unit
/// rejects reads, writes, and admin maintenance alike until re-enabled.
pub(crate) async fn require_unit_enabled(
    ctx: &RpcCtx,
    unit: RepoUnit,
    repo_id: &str,
) -> Result<(), AppError> {
    let flags = ctx.db.get_repo_unit_flags(repo_id).await.map_err(db_err)?;
    let enabled = match unit {
        RepoUnit::Issues => flags.issues_enabled,
        RepoUnit::Pulls => flags.pulls_enabled,
    };
    if enabled {
        Ok(())
    } else {
        Err(unit.disabled_error())
    }
}

/// `repo.<unit>.getEnabled` — Read+ (anonymous OK on public repos).
async fn get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
    unit: RepoUnit,
) -> Result<RepoUnitEnabledResponse, AppError> {
    let rpc_name = format!("{}.getEnabled", unit.rpc_namespace());
    let req: RepoUnitGetEnabledRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid {rpc_name} input: {e}")))?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let flags = ctx
        .db
        .get_repo_unit_flags(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let enabled = match unit {
        RepoUnit::Issues => flags.issues_enabled,
        RepoUnit::Pulls => flags.pulls_enabled,
    };
    Ok(RepoUnitEnabledResponse { enabled })
}

/// `repo.<unit>.setEnabled` — Admin only.
async fn set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
    unit: RepoUnit,
) -> Result<RepoUnitEnabledResponse, AppError> {
    let rpc_name = format!("{}.setEnabled", unit.rpc_namespace());
    let _ = require_verified(ctx).await?;
    let req: RepoUnitSetEnabledRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid {rpc_name} input: {e}")))?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    match unit {
        RepoUnit::Issues => {
            ctx.db
                .set_repo_issues_enabled(&accessible.row.id, req.enabled)
                .await
        }
        RepoUnit::Pulls => {
            ctx.db
                .set_repo_pulls_enabled(&accessible.row.id, req.enabled)
                .await
        }
    }
    .map_err(db_err)?;
    Ok(RepoUnitEnabledResponse {
        enabled: req.enabled,
    })
}

/// `repo.issues.getEnabled`.
pub async fn issues_get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    get_enabled(ctx, input, RepoUnit::Issues).await
}

/// `repo.issues.setEnabled`.
pub async fn issues_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    set_enabled(ctx, input, RepoUnit::Issues).await
}

/// `repo.pulls.getEnabled`.
pub async fn pulls_get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    get_enabled(ctx, input, RepoUnit::Pulls).await
}

/// `repo.pulls.setEnabled`.
pub async fn pulls_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    set_enabled(ctx, input, RepoUnit::Pulls).await
}
