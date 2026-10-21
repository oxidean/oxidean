//! Actions control plane (Phase 19) — parse/discover/dispatch/protocol; no in-process job execution.

pub mod artifacts;
pub mod artifacts_v2;
pub mod cache;
pub mod cache_v2;
pub mod cron;
pub mod dispatch;
pub mod eval_ctx;
pub mod events;
pub mod filters;
pub mod gitea_proto;
pub mod hooks;
pub mod job_tokens;
pub mod logs;
pub mod matrix;
pub mod oidc;
pub mod parse;
pub mod payload;
pub mod policy;
pub mod reusable;
pub mod runner_proto;
pub mod scheduler;
pub mod secrets;
pub mod statuses;
pub mod sweeps;
pub mod task_wake;
pub mod templates;
pub mod tokens;
pub mod vars;
pub mod workflow;

pub use dispatch::{dispatch_push_for_sha, enqueue_run, env_actions_enabled, notify_push_actions};
pub use events::{
    dispatch_pull_request, dispatch_pull_request_for_sha, notify_lifecycle_actions,
    notify_pull_request_actions, LifecycleEvent, PullRequestAction, PullRequestEvent,
};
pub use logs::{append_job_log, read_job_log};
pub use parse::{
    effective_contents_permission, effective_permissions, parse_workflow_yaml, ContentsPermission,
    EnvironmentSpec, JobSpec, ParseError, StepSpec, WorkflowDocument, WorkflowPermissions,
    WorkflowTriggers,
};
pub use statuses::{job_status_to_commit_state, publish_from_job_update, status_context};
pub use tokens::{mint_registration_token, mint_registration_token_for_scope};
pub use workflow::{
    discover_workflows, DiscoverError, DiscoveredWorkflow, DiscoveryFileError, WorkflowDiscovery,
    MAX_WORKFLOW_BYTES,
};

// ——— Shared RPC helpers (issue #162) — small row→public mappers + the
// treeish resolver used by `actions::templates`, `deployment`, `org::runners`
// and the `api-actions` RPC handlers.

use crate::rpc::RpcCtx;
use oxidean_core::{ActionDeploymentPublic, ActionRunnerPublic, AppError};
use oxidean_db::ActionRunnerRow;

pub fn runner_public(r: &ActionRunnerRow) -> ActionRunnerPublic {
    let labels: Vec<String> = serde_json::from_str(&r.labels_json).unwrap_or_default();
    ActionRunnerPublic {
        id: r.id.clone(),
        name: r.name.clone(),
        labels,
        repository_id: r.repository_id.clone(),
        ephemeral: r.ephemeral,
        last_online: r.last_online.clone(),
        created_at: r.created_at.clone(),
    }
}

pub fn job_status_terminal(status: &str) -> bool {
    matches!(status, "success" | "failure" | "cancelled" | "skipped")
}

pub fn deployment_public(
    d: &oxidean_db::ActionDeploymentRow,
    creator_username: Option<String>,
) -> ActionDeploymentPublic {
    ActionDeploymentPublic {
        id: d.id.clone(),
        environment: d.environment.clone(),
        run_id: d.run_id.clone(),
        job_id: d.job_id.clone(),
        sha: d.sha.clone(),
        git_ref: d.git_ref.clone(),
        url: d.url.clone(),
        task: d.task.clone(),
        description: d.description.clone(),
        payload: d
            .payload
            .as_deref()
            .and_then(|p| serde_json::from_str(p).ok()),
        creator_id: d.creator_id.clone(),
        creator_username,
        transient_environment: d.transient_environment,
        production_environment: d.production_environment,
        state: d.latest_state.clone(),
        job_status: d.job_status.clone(),
        created_at: d.created_at.clone(),
        updated_at: d.updated_at.clone(),
    }
}

/// Resolve a user-supplied ref (branch/tag/SHA) or the default branch to a SHA.
pub async fn resolve_treeish(
    ctx: &RpcCtx,
    default_branch: &str,
    git_ref: Option<&str>,
    bare: &std::path::Path,
) -> Result<String, AppError> {
    let candidate = git_ref
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("refs/heads/{default_branch}"));
    ctx.git.rev_parse(bare, &candidate).await.map_err(|e| {
        tracing::warn!(error = %e, ref_ = %candidate, "ref resolution failed");
        AppError::new("repo.actions.ref_not_found", "ref not found")
    })
}
