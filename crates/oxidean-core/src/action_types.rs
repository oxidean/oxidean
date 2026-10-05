//! Actions run/job/secrets RPC types (Phase 19 / ACT-03 / ACT-06).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunPublic {
    pub id: String,
    pub repository_id: String,
    pub workflow_path: String,
    pub workflow_name: String,
    pub event: String,
    pub head_sha: String,
    pub head_ref: String,
    pub status: String,
    pub title: String,
    /// 1-based per-workflow sequence number within the repository, computed at
    /// read time (oldest run of the workflow = 1). 0 when not computed.
    #[serde(default)]
    pub run_number: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Avatar URL for `actor` when resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_avatar_url: Option<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionJobPublic {
    pub id: String,
    pub run_id: String,
    pub job_key: String,
    pub name: String,
    pub status: String,
    pub runs_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunsListRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
    /// Optional exact status filter
    /// (`queued` / `in_progress` / `success` / `failure` / `cancelled`).
    #[serde(default)]
    pub status: Option<String>,
    /// Optional exact event filter (`push` / `pull_request` / `workflow_dispatch` / …).
    #[serde(default)]
    pub event: Option<String>,
    /// Head-branch filter — accepts `main` or `refs/heads/main`.
    #[serde(default)]
    pub branch: Option<String>,
    /// Workflow file filter, e.g. `.github/workflows/ci.yml`.
    #[serde(default)]
    pub workflow: Option<String>,
    /// Triggering actor username filter (resolved to a user id server-side).
    #[serde(default)]
    pub actor: Option<String>,
    /// Case-insensitive substring match against the run title.
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunsListResponse {
    pub runs: Vec<ActionRunPublic>,
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub per_page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunGetRequest {
    pub owner: String,
    pub name: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunGetResponse {
    pub run: ActionRunPublic,
    pub jobs: Vec<ActionJobPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionJobLogRequest {
    pub owner: String,
    pub name: String,
    pub run_id: String,
    pub job_id: String,
    /// Byte offset for incremental reads — pass the previous response's
    /// `next_offset` to receive only newly appended bytes.
    #[serde(default)]
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionJobLogResponse {
    /// Log bytes starting at the requested `offset` (lossy UTF-8).
    pub content: String,
    /// Offset to pass as `offset` for the next incremental read.
    #[serde(default)]
    pub next_offset: u64,
    /// Total log size in bytes at read time.
    #[serde(default)]
    pub size: u64,
}

/// `repo.actions.listWorkflows` — discovered workflow files at a ref.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionWorkflowsListRequest {
    pub owner: String,
    pub name: String,
    /// Optional branch/tag/SHA; defaults to the repo's default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionWorkflowPublic {
    /// Repo-relative path, e.g. `.github/workflows/ci.yml`.
    pub path: String,
    pub name: String,
    pub supports_dispatch: bool,
}

/// A workflow file that failed discovery validation and was skipped.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionWorkflowFileError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionWorkflowsListResponse {
    pub workflows: Vec<ActionWorkflowPublic>,
    /// Workflow files skipped during discovery (invalid YAML, too large, bad path).
    #[serde(default)]
    pub errors: Vec<ActionWorkflowFileError>,
    /// Resolved ref the discovery ran against.
    pub git_ref: String,
}

/// `repo.actions.dispatchWorkflow` — Write+; `workflow_dispatch` trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDispatchRequest {
    pub owner: String,
    pub name: String,
    /// Workflow path (`.github/workflows/x.yml`) or workflow name.
    pub workflow_id: String,
    /// Branch/tag/SHA to run against.
    pub git_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDispatchResponse {
    pub ok: bool,
    /// Run id when a run was enqueued.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// `repo.actions.rerunRun` / `repo.actions.cancelRun` — Write+.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunMutationRequest {
    pub owner: String,
    pub name: String,
    pub run_id: String,
    /// `rerunRun` only: requeue just this job instead of the whole run
    /// (GitHub "Re-run this job").
    #[serde(default)]
    pub job_id: Option<String>,
    /// `rerunRun` only: requeue only failed/cancelled jobs
    /// (GitHub "Re-run failed jobs").
    #[serde(default)]
    pub failed_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunMutationResponse {
    pub run: ActionRunPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSecretPublic {
    pub name: String,
    pub updated_at: String,
}

pub type ActionSecretMetaPublic = ActionSecretPublic;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSecretsListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSecretsListResponse {
    pub secrets: Vec<ActionSecretPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSecretsPutRequest {
    pub owner: String,
    pub name: String,
    pub secret_name: String,
    pub value: String,
}

pub type ActionSecretPutRequest = ActionSecretsPutRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionSecretsDeleteRequest {
    pub owner: String,
    pub name: String,
    pub secret_name: String,
}

pub type ActionSecretDeleteRequest = ActionSecretsDeleteRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoActionsEnabledRequest {
    pub owner: String,
    pub name: String,
}

pub type ActionEnabledRequest = RepoActionsEnabledRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoActionsSetEnabledRequest {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

pub type ActionSetEnabledRequest = RepoActionsSetEnabledRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoActionsEnabledResponse {
    pub enabled: bool,
}

pub type ActionEnabledResponse = RepoActionsEnabledResponse;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminActionsCreateRegistrationTokenResponse {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRegistrationTokenResponse {
    pub token: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRunnerPublic {
    pub id: String,
    pub name: String,
    pub labels: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_id: Option<String>,
    pub ephemeral: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_online: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminActionsListRunnersResponse {
    pub runners: Vec<ActionRunnerPublic>,
}

pub type ActionListRunnersResponse = AdminActionsListRunnersResponse;
