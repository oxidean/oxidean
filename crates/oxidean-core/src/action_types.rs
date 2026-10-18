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
    /// Re-run counter — 1 for the first attempt (GitHub `run_attempt`).
    #[serde(default)]
    pub attempt: i64,
    /// Why the run is held when `status` is `waiting_approval` — currently
    /// always `"fork_approval"`; `None` otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_reason: Option<String>,
    /// The caller may release a fork-approval hold (`approveRun` → Write+).
    #[serde(default)]
    pub can_approve: bool,
}

/// One approve/reject decision recorded against an environment-held job
/// (`action_deployment_reviews`, 0061).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDeploymentReviewPublic {
    /// Username of the reviewer who decided.
    pub reviewer: String,
    /// `'approved'` | `'rejected'`.
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionJobPublic {
    pub id: String,
    pub run_id: String,
    pub job_key: String,
    pub name: String,
    pub status: String,
    pub runs_on: Vec<String>,
    /// `needs:` upstream job keys (scheduling DAG).
    #[serde(default)]
    pub needs: Vec<String>,
    /// Expanded matrix values for this leg (`{}` for non-matrix jobs).
    #[serde(default)]
    pub matrix: serde_json::Value,
    /// Resolved `environment:` name when the job targets one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    /// Claim counter — bumped on requeue (GitHub `run_attempt` per leg).
    #[serde(default)]
    pub attempt: i64,
    /// Why a `waiting`/`waiting_approval` job is held:
    /// `"fork_approval"` | `"environment_review"` | `"environment_wait_timer"`
    /// | `"needs"`. `None` for queued/in-flight/terminal jobs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_reason: Option<String>,
    /// The caller may review this job's deployment hold (named required
    /// reviewer, or Admin overriding reviewer membership — and not the run's
    /// triggering actor when the env sets `prevent_self_review`).
    #[serde(default)]
    pub can_review: bool,
    /// Deployment-review history for this job (newest last).
    #[serde(default)]
    pub reviews: Vec<ActionDeploymentReviewPublic>,
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

/// `repo.actions.jobAnnotations` — annotations a job emitted through the
/// `::error|warning|notice::` workflow commands (GitHub's run/job
/// annotation surface). Fields follow GitHub's check-run annotation shape.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionAnnotationPublic {
    /// `error` | `warning` | `notice`.
    pub annotation_level: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_column: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionJobAnnotationsRequest {
    pub owner: String,
    pub name: String,
    /// Restrict to one job; absent lists every annotation on the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionJobAnnotationsResponse {
    pub annotations: Vec<ActionAnnotationPublic>,
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

/// One declared `on.workflow_dispatch.inputs.<name>` entry — drives the
/// dispatch form's typed controls.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDispatchInputPublic {
    pub name: String,
    /// `string` | `choice` | `boolean` | `number` | `environment`.
    #[serde(rename = "type")]
    pub input_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Declared default rendered for the control's initial value. `Value`
    /// because `boolean`/`number` defaults are non-string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// `choice` options; empty for other types.
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionWorkflowPublic {
    /// Repo-relative path, e.g. `.github/workflows/ci.yml`.
    pub path: String,
    pub name: String,
    pub supports_dispatch: bool,
    /// GitHub "disabled" workflow state — no trigger (events, schedule,
    /// dispatch, `workflow_call`) runs it until re-enabled.
    #[serde(default)]
    pub disabled: bool,
    /// Declared `workflow_dispatch` inputs; empty when dispatch is off.
    #[serde(default)]
    pub inputs: Vec<ActionDispatchInputPublic>,
}

/// `repo.actions.setWorkflowEnabled` — `{owner, name, path, enabled}`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionSetWorkflowEnabledRequest {
    pub owner: String,
    pub name: String,
    pub path: String,
    pub enabled: bool,
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

/// `repo.actions.dispatchRepository` — Write+; `repository_dispatch` trigger.
/// Mirrors GitHub `POST /repos/{owner}/{repo}/dispatches`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDispatchRepositoryRequest {
    pub owner: String,
    pub name: String,
    /// Activity type the run records — `on: repository_dispatch` `types:`
    /// filters match it (GitHub `event_type`).
    pub event_type: String,
    /// Arbitrary JSON echoed verbatim as `github.event.client_payload`.
    #[serde(default)]
    pub client_payload: Option<serde_json::Value>,
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
    /// `rerunRun` only: stamp `debug_logging` on the requeued jobs —
    /// GitHub's "enable debug logging" checkbox; surfaced to the runner as
    /// `ACTIONS_STEP_DEBUG`/`ACTIONS_RUNNER_DEBUG` at claim.
    #[serde(default)]
    pub debug: Option<bool>,
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
    /// Reviewer user ids — write accepts ids or usernames (normalized to
    /// ids); read additionally resolves `required_reviewer_usernames`.
    #[serde(default)]
    pub required_reviewers: Vec<String>,
    /// Resolved usernames for `required_reviewers` (read-side only — ignored
    /// on write; the UI displays these).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_reviewer_usernames: Vec<String>,
    /// GitHub parity: when true the run's triggering actor may not review the
    /// deployment even when named in `required_reviewers` (or an Admin).
    #[serde(default)]
    pub prevent_self_review: bool,
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
    /// Present for environment-job deployments; NULL for API-created ones.
    pub run_id: Option<String>,
    pub job_id: Option<String>,
    pub sha: String,
    /// Serializes as `ref` (GitHub `deployment.ref`).
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub url: Option<String>,
    /// GitHub `deployment.task` — `deploy` for environment jobs.
    pub task: String,
    pub description: String,
    /// Free-form JSON payload (GitHub `deployment.payload`).
    pub payload: Option<serde_json::Value>,
    pub creator_id: Option<String>,
    pub creator_username: Option<String>,
    pub transient_environment: bool,
    pub production_environment: bool,
    /// Latest `action_deployment_statuses.state` — the Deployments-API
    /// `state` projection (NULL until the first status lands).
    pub state: Option<String>,
    /// Derived from the backing job's status (coalesced `waiting_approval`).
    pub job_status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// `deployment.list` — repo-scoped listing (GitHub REST `GET /deployments`
/// parity: `sha`, `ref`, `task`, `environment` filters).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentListRequest {
    pub owner: String,
    pub name: String,
    pub sha: Option<String>,
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    pub task: Option<String>,
    pub environment: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentListResponse {
    pub deployments: Vec<ActionDeploymentPublic>,
}

/// `deployment.get` / `deployment.delete` — one deployment by id.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentRefRequest {
    pub owner: String,
    pub name: String,
    pub id: String,
}

/// `deployment.create` — GitHub REST `POST /deployments` parity.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentCreateRequest {
    pub owner: String,
    pub name: String,
    /// Branch name, `refs/...`, tag, or sha — resolved server-side.
    #[serde(rename = "ref")]
    pub git_ref: String,
    /// Task identifier (GitHub default `deploy`).
    pub task: Option<String>,
    /// Merge the default branch into the ref before deploying — creates a
    /// merge commit on the ref (GitHub default `true`; only applies to
    /// branch refs that aren't the default branch).
    pub auto_merge: Option<bool>,
    /// Commit-status contexts that must be `success`. Omitted verifies
    /// every context present on the sha; `[]` skips verification (GitHub).
    pub required_contexts: Option<Vec<String>>,
    /// Free-form JSON stored on the deployment.
    pub payload: Option<serde_json::Value>,
    /// Environment name (GitHub default `production`).
    pub environment: Option<String>,
    pub description: Option<String>,
    /// Transient envs never auto-inactivate prior deployments.
    pub transient_environment: Option<bool>,
    /// Defaults from the environment's protection rules when unset.
    pub production_environment: Option<bool>,
}

pub type DeploymentCreateResponse = ActionDeploymentPublic;
pub type DeploymentGetResponse = ActionDeploymentPublic;

/// `deployment.status.create` — GitHub REST `POST
/// /deployments/{id}/statuses` parity.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentStatusCreateRequest {
    pub owner: String,
    pub name: String,
    pub deployment_id: String,
    /// `error`|`failure`|`inactive`|`in_progress`|`pending`|`queued`|
    /// `success`|`waiting` (GitHub vocabulary).
    pub state: String,
    pub description: Option<String>,
    /// Overrides the deployment's environment name on this status.
    pub environment: Option<String>,
    pub environment_url: Option<String>,
    /// Logs/result link (GitHub `log_url`; `target_url` accepted as alias).
    pub log_url: Option<String>,
    pub target_url: Option<String>,
    /// `success` statuses mark prior env deployments `inactive` (GitHub
    /// default `true`).
    pub auto_inactive: Option<bool>,
}

/// One `action_deployment_statuses` row in GitHub's status shape.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentStatusPublic {
    pub id: String,
    pub deployment_id: String,
    pub state: String,
    pub description: String,
    pub environment: Option<String>,
    pub environment_url: Option<String>,
    pub target_url: Option<String>,
    pub log_url: Option<String>,
    pub creator_id: Option<String>,
    pub creator_username: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentStatusListRequest {
    pub owner: String,
    pub name: String,
    pub deployment_id: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentStatusListResponse {
    pub statuses: Vec<DeploymentStatusPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct DeploymentDeleteResponse {
    pub ok: bool,
}

/// `repo.actions.listDeployments` — deployment records for a run, or for the
/// repo (optionally narrowed to one environment) when `run_id` is absent.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDeploymentsListRequest {
    pub owner: String,
    pub name: String,
    #[serde(default)]
    pub run_id: Option<String>,
    /// Narrow a repo-level listing to one environment (ignored with `run_id`).
    #[serde(default)]
    pub environment: Option<String>,
    /// Repo-level listing cap (default 100, max 500).
    #[serde(default)]
    pub limit: Option<u32>,
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

/// `repo.actions.reviewDeployment` — approve or reject an environment-held
/// job, mirroring GitHub's `pending_deployments` review. Write+ plus reviewer
/// membership when the environment names `required_reviewers` (Admin
/// overrides); `prevent_self_review` blocks the run's triggering actor.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDeploymentReviewRequest {
    pub owner: String,
    pub name: String,
    pub job_id: String,
    /// `'approved'` | `'rejected'` — rejection fails the job and skips its
    /// dependents (GitHub `state`).
    pub state: String,
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionDeploymentReviewResponse {
    pub job: ActionJobPublic,
}

/// Public artifact metadata (`action_artifacts`) — the payload streams from
/// the download route, never through RPC.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionArtifactPublic {
    pub id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<String>,
    pub name: String,
    pub size_bytes: i64,
    pub digest_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

/// `repo.actions.listArtifacts` — Read+; every artifact recorded for a run.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionArtifactsListRequest {
    pub owner: String,
    pub name: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionArtifactsListResponse {
    pub artifacts: Vec<ActionArtifactPublic>,
}

/// `repo.actions.deleteArtifact` — Write+; removes the record and payload.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionArtifactDeleteRequest {
    pub owner: String,
    pub name: String,
    pub artifact_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionArtifactDeleteResponse {
    pub deleted: bool,
}

// --- actions/cache service (P14): repo management surface ---

/// Public cache-entry metadata (`action_caches`) — payloads stream from the
/// `_apis/artifactcache` download route, never through RPC.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionCachePublic {
    pub id: String,
    pub key: String,
    pub version: String,
    /// Branch scope the entry was saved under.
    pub scope: String,
    pub size_bytes: i64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub last_accessed_at: String,
}

/// `repo.actions.listCaches` — Read+; committed entries + usage rollup.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionCachesListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionCachesListResponse {
    pub caches: Vec<ActionCachePublic>,
    /// Sum of committed `size_bytes` — the GitHub `cache/usage` answer.
    pub total_size_bytes: i64,
}

/// `repo.actions.deleteCache` — Write+; removes the record and payload.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionCacheDeleteRequest {
    pub owner: String,
    pub name: String,
    pub cache_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionCacheDeleteResponse {
    pub deleted: bool,
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

// --- configuration variables (`vars` context) — plaintext, values ARE
// returned on GET (they are not secrets). ---

/// Public variable view — name + value (plaintext by design) + updated_at.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionVariablePublic {
    pub name: String,
    pub value: String,
    pub updated_at: String,
}

/// `repo.actions.variables.list` — Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionVariablesListRequest {
    pub owner: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionVariablesListResponse {
    pub variables: Vec<ActionVariablePublic>,
}

/// `repo.actions.variables.put` — Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionVariablesPutRequest {
    pub owner: String,
    pub name: String,
    pub variable_name: String,
    pub value: String,
}

/// `repo.actions.variables.delete` — Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ActionVariablesDeleteRequest {
    pub owner: String,
    pub name: String,
    pub variable_name: String,
}

/// `repo.actions.environments.variablesList` — Admin; values included.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentVariablesListRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
}

/// `repo.actions.environments.variablesPut` — Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentVariablePutRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
    pub variable_name: String,
    pub value: String,
}

/// `repo.actions.environments.variablesDelete` — Admin.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RepoActionsEnvironmentVariableDeleteRequest {
    pub owner: String,
    pub name: String,
    pub environment: String,
    pub variable_name: String,
}

/// Public org variable view — value + visibility (plaintext by design).
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgVariablePublic {
    pub name: String,
    pub value: String,
    /// 'all' | 'private' | 'selected'.
    pub visibility: String,
    pub updated_at: String,
}

/// `org.actions.variables.list` — Admin+.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsVariablesListRequest {
    pub slug: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsVariablesListResponse {
    pub variables: Vec<OrgVariablePublic>,
}

/// `org.actions.variables.put` — Admin+. `visibility` defaults to 'all';
/// `repository_ids` replaces the selected-repo set when 'selected'.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsVariablePutRequest {
    pub slug: String,
    pub variable_name: String,
    pub value: String,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub repository_ids: Option<Vec<String>>,
}

/// `org.actions.variables.delete` — Admin+.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsVariableDeleteRequest {
    pub slug: String,
    pub variable_name: String,
}

/// `org.actions.variables.listRepos` — Admin+; repo ids in the selected set.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsVariableReposRequest {
    pub slug: String,
    pub variable_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OrgActionsVariableReposResponse {
    pub repository_ids: Vec<String>,
}
