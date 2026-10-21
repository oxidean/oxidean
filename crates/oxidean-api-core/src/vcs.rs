//! `vcs.*` shared slot — `VcsSlot`/`SharedVcs`/`vcs_api` stay in core:
//! `AppState`/`RpcCtx` hold the slot and `jobs::schedule` consumes it (#162).
//! The `vcs.*` handlers moved to `oxidean-api-vcs`.

use std::sync::Arc;

use oxidean_core::{AppError, VcsErrorCode};
use oxidean_vcs::api::VcsApi;

use crate::rpc::RpcCtx;

/// The assembled `VcsApi` + its objects dir — the `AppState`/`RpcCtx` slot.
/// `None` means the instance runs with VCS storage unconfigured (the
/// `OXIDEAN_VCS_*` bootstrap refused): every procedure returns
/// `vcs.disabled`, which is also what an unset repo flag produces — the same
/// wire behavior on both "repo not enabled" and "instance not enabled".
pub struct VcsSlot {
    pub api: VcsApi,
    /// Objects dir — factory-reset sweeps it (it's not under `repos_dir`).
    pub objects_dir: std::path::PathBuf,
    /// `OXIDEAN_VCS_GC_INTERVAL_SECS` — the scheduled GC period (`0` disables
    /// the job; the frozen default is 3600).
    pub gc_interval_secs: u64,
}

pub fn vcs_api(ctx: &RpcCtx) -> Result<&VcsApi, AppError> {
    ctx.vcs
        .as_ref()
        .map(|slot| &slot.api)
        .ok_or_else(|| VcsErrorCode::Disabled.app_error("VCS is not enabled for this repository"))
}

/// `Arc<VcsSlot>` for `AppState`/`RpcCtx` — the slot is shared across ctx
/// clones because `VcsApi` holds handle-only state (db pool + dir).
pub type SharedVcs = Arc<VcsSlot>;
