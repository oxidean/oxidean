//! Actions run/job/secrets RPC types (Phase 19 / ACT-03 / ACT-06).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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
    /// `$GITHUB_STEP_SUMMARY` markdown reported at terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionRunsListResponse {
    pub runs: Vec<ActionRunPublic>,
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub page: u32,
    #[serde(default)]
    pub per_page: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionRunGetRequest {
    pub owner: String,
    pub name: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionRunGetResponse {
    pub run: ActionRunPublic,
    pub jobs: Vec<ActionJobPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionJobLogRequest {
    pub owner: String,
    pub name: String,
    pub run_id: String,
    pub job_id: String,
    /// Byte offset for incremental reads — pass the previous response's
    /// `next_offset` to receive only newly appended bytes.
    #[serde(default)]
    pub offset: Option<u64>,
    /// Live-tail long-poll: when no bytes are available past `offset`, hold
    /// the request up to this many milliseconds (server caps at 30s) and
    /// return as soon as new bytes land or the job goes terminal. `0`/absent
    /// keeps the old immediate-return behavior.
    #[serde(default)]
    pub wait_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionJobLogResponse {
    /// Log bytes starting at the requested `offset` (lossy UTF-8).
    pub content: String,
    /// Offset to pass as `offset` for the next incremental read.
    #[serde(default)]
    pub next_offset: u64,
    /// Total log size in bytes at read time.
    #[serde(default)]
    pub size: u64,
    /// The job reached a terminal status — callers can stop polling.
    /// `false` on servers that predate the flag.
    #[serde(default)]
    pub done: bool,
}

/// `repo.actions.listWorkflows` — discovered workflow files at a ref.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionWorkflowsListRequest {
    pub owner: String,
    pub name: String,
    /// Optional branch/tag/SHA; defaults to the repo's default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionWorkflowPublic {
    /// Repo-relative path, e.g. `.github/workflows/ci.yml`.
    pub path: String,
    pub name: String,
    pub supports_dispatch: bool,
}

/// A workflow file that failed discovery validation and was skipped.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionWorkflowFileError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionWorkflowsListResponse {
    pub workflows: Vec<ActionWorkflowPublic>,
    /// Workflow files skipped during discovery (invalid YAML, too large, bad path).
    #[serde(default)]
    pub errors: Vec<ActionWorkflowFileError>,
    /// Resolved ref the discovery ran against.
    pub git_ref: String,
}

/// `repo.actions.dispatchWorkflow` — Write+; `workflow_dispatch` trigger.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDispatchRequest {
    pub owner: String,
    pub name: String,
    /// Workflow path (`.github/workflows/x.yml`) or workflow name.
    pub workflow_id: String,
    /// Branch/tag/SHA to run against.
    pub git_ref: String,
    /// `on.workflow_dispatch.inputs` values — validated against the declared
    /// input types at dispatch; missing keys take their declared `default`.
    #[serde(default)]
    pub inputs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDispatchResponse {
    pub ok: bool,
    /// Run id when a run was enqueued.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// `repo.actions.rerunRun` / `repo.actions.cancelRun` — Write+.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionRunMutationResponse {
    pub run: ActionRunPublic,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionSecretPublic {
    pub name: String,
    pub updated_at: String,
}

pub type ActionSecretMetaPublic = ActionSecretPublic;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionSecretsListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionSecretsListResponse {
    pub secrets: Vec<ActionSecretPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionSecretsPutRequest {
    pub owner: String,
    pub name: String,
    pub secret_name: String,
    pub value: String,
}

pub type ActionSecretPutRequest = ActionSecretsPutRequest;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionSecretsDeleteRequest {
    pub owner: String,
    pub name: String,
    pub secret_name: String,
}

pub type ActionSecretDeleteRequest = ActionSecretsDeleteRequest;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnabledRequest {
    pub owner: String,
    pub name: String,
}

pub type ActionEnabledRequest = RepoActionsEnabledRequest;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsSetEnabledRequest {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

pub type ActionSetEnabledRequest = RepoActionsSetEnabledRequest;

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnabledResponse {
    pub enabled: bool,
}

pub type ActionEnabledResponse = RepoActionsEnabledResponse;

/// `repo.actions.getPullRequestTarget` / `setPullRequestTarget` — Admin;
/// repository opt-in for the `pull_request_target` trigger (trusted-context
/// PR workflows). Read: Read+; Write: Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsPullRequestTargetRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsSetPullRequestTargetRequest {
    pub owner: String,
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsPullRequestTargetResponse {
    pub enabled: bool,
}

/// `repo.actions.getForkPrApproval` / `setForkPrApproval` — Admin; fork PR
/// run approval policy: `never` | `first-time` | `always`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsForkPrApprovalRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsSetForkPrApprovalRequest {
    pub owner: String,
    pub name: String,
    /// `never` (default), `first-time`, or `always`.
    pub policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsForkPrApprovalResponse {
    pub policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminActionsCreateRegistrationTokenResponse {
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionRegistrationTokenResponse {
    pub token: String,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminActionsListRunnersResponse {
    pub runners: Vec<ActionRunnerPublic>,
}

pub type ActionListRunnersResponse = AdminActionsListRunnersResponse;

/// Provenance of a workflow template in the `repo.actions.workflowTemplates`
/// catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowTemplateProvenance {
    /// Vendored from actions/starter-workflows.
    Builtin,
    /// Instance-managed (admin CRUD, `instance_workflow_templates` table).
    Instance,
    /// Published by the repository owner's `.github` repository under
    /// `workflow-templates/`.
    Org,
}

/// A workflow template in the catalog (GitHub "New workflow" parity).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowTemplateSummary {
    /// `builtin:<dir>/<file-stem>` | `instance:<slug>` | `org:<file-stem>`.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Properties `iconName`; absent when the source provides none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_name: Option<String>,
    /// `/api/…/template-icons/<icon_name>.svg` when the icon is servable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    /// Root-tree glob patterns used for `suggested` detection.
    #[serde(default)]
    pub file_patterns: Vec<String>,
    /// Template file name — the default create target is
    /// `.github/workflows/<filename>`.
    pub filename: String,
    pub provenance: WorkflowTemplateProvenance,
    /// Human source hint — `@<org>/.github` for org templates, slug for
    /// instance templates; absent for builtin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_label: Option<String>,
    /// True when any `file_patterns` entry matched the repository root tree.
    #[serde(default)]
    pub suggested: bool,
    /// Distinct `uses:` refs the bundled runner cannot execute (anything not
    /// `actions/checkout*`).
    #[serde(default)]
    pub unsupported_actions: Vec<String>,
    /// Set when the template fails `parse_workflow_yaml`; content is still
    /// returned verbatim.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
}

/// `repo.actions.workflowTemplates.list` — merged catalog for a repository.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowTemplatesListRequest {
    pub owner: String,
    pub name: String,
    /// Optional branch/tag/SHA for suggestion matching; defaults to the
    /// repository's default branch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    /// Exact category filter (case-insensitive).
    #[serde(default)]
    pub category: Option<String>,
    /// Provenance filter: `builtin` | `instance` | `org`.
    #[serde(default)]
    pub source: Option<String>,
    /// Case-insensitive substring match on name/description.
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub per_page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowTemplatesListResponse {
    /// Subset of `templates` with `suggested == true`.
    #[serde(default)]
    pub suggested: Vec<WorkflowTemplateSummary>,
    #[serde(default)]
    pub templates: Vec<WorkflowTemplateSummary>,
    /// Distinct categories across `templates` (GitHub display order where
    /// known, then alphabetical).
    #[serde(default)]
    pub categories: Vec<String>,
    /// Filtered template count before any pagination.
    #[serde(default)]
    pub total_count: u64,
}

/// `repo.actions.workflowTemplates.get` — full template content.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowTemplateGetRequest {
    pub owner: String,
    pub name: String,
    /// Catalog id (`builtin:ci/rust`, `instance:<slug>`, `org:<file-stem>`).
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowTemplateGetResponse {
    pub template: WorkflowTemplateSummary,
    /// Raw template content with `$default-branch` substituted for the
    /// repository's default branch.
    pub content: String,
}

/// `repo.actions.workflows.create` — draft a workflow file from a catalog
/// template (`template_id`) or authored `content` onto a branch or an open PR.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WorkflowCreateRequest {
    pub owner: String,
    pub name: String,
    /// Catalog template id. Exactly one of `template_id` / `content`.
    #[serde(default)]
    pub template_id: Option<String>,
    /// Authored workflow YAML. Exactly one of `template_id` / `content`.
    #[serde(default)]
    pub content: Option<String>,
    /// Destination path. Defaults to `.github/workflows/<template filename>`
    /// for templates; required for authored content. Must be
    /// `.github/workflows/<flat-name>.yml|.yaml`.
    #[serde(default)]
    pub path: Option<String>,
    /// Commit message (default: `ci: add <filename> workflow`).
    #[serde(default)]
    pub message: Option<String>,
    /// When set, commit onto this open PR's head branch (same-repo PRs only).
    #[serde(default)]
    pub pull_number: Option<i64>,
    #[serde(flatten)]
    pub target: crate::repo_types::RepoFileCommitOptions,
}

/// Instance-managed workflow template as stored in `instance_workflow_templates`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct InstanceWorkflowTemplatePublic {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_name: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub file_patterns: Vec<String>,
    pub filename: String,
    #[serde(default)]
    pub content: String,
    pub enabled: bool,
    pub created_by_user_id: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
}

/// `admin.workflowTemplates.list` response.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminWorkflowTemplatesListResponse {
    #[serde(default)]
    pub templates: Vec<InstanceWorkflowTemplatePublic>,
}

/// `admin.workflowTemplates.create` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminWorkflowTemplateCreateRequest {
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub icon_name: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub file_patterns: Vec<String>,
    /// Default `<slug>.yml`.
    #[serde(default)]
    pub filename: Option<String>,
    pub content: String,
}

/// `admin.workflowTemplates.update` input — only set fields change.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminWorkflowTemplateUpdateRequest {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon_name: Option<Option<String>>,
    #[serde(default)]
    pub categories: Option<Vec<String>>,
    #[serde(default)]
    pub file_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

/// `admin.workflowTemplates.setEnabled` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminWorkflowTemplateSetEnabledRequest {
    pub id: String,
    pub enabled: bool,
}

/// `admin.workflowTemplates.delete` input.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AdminWorkflowTemplateDeleteRequest {
    pub id: String,
}

/// `repo.actions.getDefaultPermissions` / `setDefaultPermissions` — the
/// permission floor applied when a workflow declares no `permissions:` block:
/// `read` (contents+packages read) or `write` (all scopes write).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsDefaultPermissionsRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsSetDefaultPermissionsRequest {
    pub owner: String,
    pub name: String,
    /// `read` (default) or `write`.
    pub default_permissions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsDefaultPermissionsResponse {
    pub default_permissions: String,
}

/// Environment protection rules — GitHub's `protection_rules` shape
/// (`required_reviewers` = user ids, `wait_timer_minutes`,
/// `deployment_branches` = branch allowlist). All fields optional.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionEnvironmentProtection {
    #[serde(default)]
    pub wait_timer_minutes: i64,
    #[serde(default)]
    pub required_reviewers: Vec<String>,
    #[serde(default)]
    pub deployment_branches: Vec<String>,
}

/// Public environment record (`repo.actions.environments.*`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionEnvironmentPublic {
    pub name: String,
    pub protection: ActionEnvironmentProtection,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentsListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentsListResponse {
    pub environments: Vec<ActionEnvironmentPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentGetRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentResponse {
    pub environment: ActionEnvironmentPublic,
}

/// `repo.actions.upsertEnvironment` — create or replace an environment's
/// protection rules (Admin). Missing env rows are created on save.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentUpsertRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
    #[serde(default)]
    pub protection: ActionEnvironmentProtection,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentDeleteRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentDeleteResponse {
    pub deleted: bool,
}

/// One recorded environment deployment (`action_deployments`).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDeploymentPublic {
    pub id: String,
    pub environment: String,
    pub run_id: String,
    pub job_id: String,
    pub sha: String,
    pub git_ref: String,
    pub url: Option<String>,
    /// Derived from the backing job's status (coalesced `waiting_approval`).
    pub job_status: String,
    pub created_at: String,
}

/// `repo.actions.listDeployments` — deployment records for a run.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDeploymentsListRequest {
    pub owner: String,
    pub name: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDeploymentsListResponse {
    pub deployments: Vec<ActionDeploymentPublic>,
}

/// `repo.actions.approveJob` — release one approval-held job (deployment
/// review). Write+; when the environment names `required_reviewers` the
/// caller must be one of them or an Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionJobApproveRequest {
    pub owner: String,
    pub name: String,
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionJobApproveResponse {
    pub job: ActionJobPublic,
}

// --- Phase 8: org + environment secrets ---

/// `repo.actions.environments.secretsList` — names only, never values.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentSecretsListRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
}

/// `repo.actions.environments.secretsPut` — Admin; value never returned.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentSecretPutRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
    pub secret_name: String,
    pub value: String,
}

/// `repo.actions.environments.secretsDelete` — Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentSecretDeleteRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
    pub secret_name: String,
}

/// Public metadata for an org Actions secret — never the value.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgSecretMetaPublic {
    pub name: String,
    /// 'all' | 'private' | 'selected'.
    pub visibility: String,
    pub updated_at: String,
}

/// `org.actions.secrets.list` — Admin+; names + visibility only.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsSecretsListRequest {
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsSecretsListResponse {
    pub secrets: Vec<OrgSecretMetaPublic>,
}

/// `org.actions.secrets.put` — Admin+. `visibility` defaults to 'all';
/// `repository_ids` replaces the selected-repo set when visibility is
/// 'selected'.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsSecretPutRequest {
    pub slug: String,
    pub secret_name: String,
    pub value: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub repository_ids: Option<Vec<String>>,
}

/// `org.actions.secrets.delete` — Admin+.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsSecretDeleteRequest {
    pub slug: String,
    pub secret_name: String,
}

/// `org.actions.secrets.listRepos` — Admin+; repo ids in the selected set.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsSecretReposRequest {
    pub slug: String,
    pub secret_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsSecretReposResponse {
    pub repository_ids: Vec<String>,
}
