//! Actions control plane (Phase 19) — parse/discover/dispatch/protocol; no in-process job execution.

pub mod dispatch;
pub mod events;
pub mod hooks;
pub mod logs;
pub mod parse;
pub mod rpc;
pub mod runner_proto;
pub mod secrets;
pub mod statuses;
pub mod tokens;
pub mod workflow;

pub use dispatch::{
    dispatch_push_for_sha, enqueue_run, env_actions_enabled, notify_push_actions,
};
pub use events::{
    dispatch_pull_request, dispatch_pull_request_for_sha, notify_pull_request_actions,
    PullRequestAction, PullRequestEvent,
};
pub use logs::{append_job_log, read_job_log};
pub use parse::{
    parse_workflow_yaml, JobSpec, ParseError, StepSpec, WorkflowDocument, WorkflowTriggers,
};
pub use statuses::{job_status_to_commit_state, publish_from_job_update, status_context};
pub use tokens::mint_registration_token;
pub use workflow::{
    discover_workflows, DiscoverError, DiscoveredWorkflow, DiscoveryFileError,
    WorkflowDiscovery, MAX_WORKFLOW_BYTES,
};
