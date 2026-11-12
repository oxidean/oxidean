//! `experiments.list` / `experiments.set` — per-user experiment flags
//! (WI-0001). The `oxidean-core` registry is authoritative: `list` projects
//! it with the caller's enabled state (anonymous → all off), `set` validates
//! the key before persisting so stale clients can't write unknown rows.

use std::collections::BTreeSet;

use oxidean_core::{
    AppError, ExperimentPublic, ExperimentSetRequest, ExperimentsListResponse, UserPublic,
    EXPERIMENTS,
};

use oxidean_api_core::auth::local::user_to_public;
use oxidean_api_core::rpc::RpcCtx;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("experiments db error: {e}");
        AppError::new("experiments.internal", "experiment operation failed")
    }
}

fn require_session(
    ctx: &RpcCtx,
) -> Result<&oxidean_api_core::auth::session::ResolvedSession, AppError> {
    ctx.session
        .as_ref()
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))
}

/// `experiments.list` — the registry plus the caller's enabled state.
/// Callable without a session; anonymous callers see every flag disabled.
pub async fn list(ctx: &RpcCtx) -> Result<ExperimentsListResponse, AppError> {
    let enabled: BTreeSet<String> = match &ctx.session {
        Some(session) => ctx
            .db
            .list_user_experiments(&session.user_id)
            .await
            .map_err(db_err)?
            .into_iter()
            .collect(),
        None => BTreeSet::new(),
    };
    Ok(ExperimentsListResponse {
        experiments: EXPERIMENTS
            .iter()
            .map(|def| ExperimentPublic {
                key: def.key.to_string(),
                title: def.title.to_string(),
                description: def.description.to_string(),
                status: def.status,
                since_version: def.since_version.to_string(),
                enabled: enabled.contains(def.key),
            })
            .collect(),
    })
}

/// `experiments.set` — toggle one registry flag for the calling user.
/// Returns the updated `UserPublic` (mirrors `user.update_profile`).
pub async fn set(ctx: &RpcCtx, input: serde_json::Value) -> Result<UserPublic, AppError> {
    let session = require_session(ctx)?;
    let req: ExperimentSetRequest = serde_json::from_value(input)
        .map_err(|_| AppError::new("rpc.invalid_input", "expected {key, enabled}"))?;
    let def = oxidean_core::experiment(&req.key)
        .ok_or_else(|| AppError::new("experiments.unknown_key", "unknown experiment flag"))?;
    ctx.db
        .set_user_experiment(&session.user_id, def.key, req.enabled)
        .await
        .map_err(db_err)?;
    let user = ctx
        .db
        .find_user_by_id(&session.user_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("auth.unauthenticated", "not authenticated"))?;
    Ok(user_to_public(&ctx.db, &user).await)
}
