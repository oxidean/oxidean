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
pub mod rpc;
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
