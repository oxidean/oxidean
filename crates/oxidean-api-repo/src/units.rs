//! `repo.<unit>.{get,set}Enabled` handlers (issue #162). The unit-flag
//! machinery lives in `oxidean_api_core::repo::units`.

use oxidean_api_core::repo::units::{get_enabled, set_enabled, RepoUnit};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{AppError, RepoUnitEnabledResponse};

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

/// `repo.wiki.getEnabled`.
pub async fn wiki_get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    get_enabled(ctx, input, RepoUnit::Wiki).await
}

/// `repo.wiki.setEnabled`.
pub async fn wiki_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    set_enabled(ctx, input, RepoUnit::Wiki).await
}

/// `repo.projects.getEnabled`.
pub async fn projects_get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    get_enabled(ctx, input, RepoUnit::Projects).await
}

/// `repo.projects.setEnabled`.
pub async fn projects_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    set_enabled(ctx, input, RepoUnit::Projects).await
}

/// `repo.discussions.getEnabled`.
pub async fn discussions_get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    get_enabled(ctx, input, RepoUnit::Discussions).await
}

/// `repo.discussions.setEnabled`.
pub async fn discussions_set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoUnitEnabledResponse, AppError> {
    set_enabled(ctx, input, RepoUnit::Discussions).await
}
