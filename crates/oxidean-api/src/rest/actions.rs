//! GitHub-compatible Actions artifact routes under `/api/v1` — the surface
//! `download-artifact`'s cross-run path (`github-token` + `run-id`), `gh
//! run download`, and other Octokit clients reach through `api_url`.
//!
//! JSON endpoints dispatch through the typed RPC (`repo.actions.*`) exactly
//! like every other `/api/v1` route. The `/zip` stream shares the
//! `repo.actions.getArtifact` auth check, then serves the stored blob —
//! builtin runner-proto uploads persist tar.gz and are repacked to zip on
//! the fly so `unzip`-based clients see GitHub's wire format.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, patch, post, put};
use axum::Json;
use schemars::SchemaGenerator;
use serde::Deserialize;
use serde_json::{json, Value};

use std::collections::HashMap;

use oxidean_core::AppError;

use super::spec::{BodySpec, RespSpec, RouteDef};
use super::{call, call_map, ctx_for, dispatch, repo_ref, status_for_error, RepoPath};
use crate::app::AppState;
use crate::routes::release_assets::sanitize_filename;
use crate::rpc::RpcCtx;
use oxidean_core::RpcResponse;

#[derive(Deserialize, schemars::JsonSchema)]
struct ArtifactsQuery {
    /// Filter by exact artifact name (GitHub `name` query param).
    name: Option<String>,
    /// Page size — GitHub default 30, max 100.
    per_page: Option<u32>,
    /// 1-based page number.
    page: Option<u32>,
}

#[derive(Deserialize)]
struct RunArtifactsPath {
    owner: String,
    repo: String,
    run_id: String,
}

#[derive(Deserialize)]
struct ArtifactPath {
    owner: String,
    repo: String,
    artifact_id: String,
}

#[derive(Deserialize)]
struct RunPath {
    owner: String,
    repo: String,
    run_id: String,
}

impl RunPath {
    fn ref_input(&self) -> Value {
        json!({
            "owner": self.owner,
            "name": self.repo,
            "run_id": self.run_id,
        })
    }
}

#[derive(Deserialize)]
struct JobPath {
    owner: String,
    repo: String,
    job_id: String,
}

#[derive(Deserialize)]
struct WorkflowPath {
    owner: String,
    repo: String,
    /// Numeric id, workflow filename, full path, or workflow name.
    workflow_id: String,
}

/// `GET /actions/runs` filters — the subset GitHub documents plus the
/// pagination pair. `created`/`exclude_pull_requests`/`check_suite_id` are
/// accepted and ignored (no backing filter).
#[derive(Deserialize, schemars::JsonSchema)]
struct RunsQuery {
    actor: Option<String>,
    branch: Option<String>,
    event: Option<String>,
    /// `queued`/`in_progress`/`waiting`/`completed` or a conclusion name.
    status: Option<String>,
    per_page: Option<u32>,
    page: Option<u32>,
}

/// `POST …/dispatches` body — `{ref, inputs}`.
#[derive(Deserialize, schemars::JsonSchema)]
struct DispatchBody {
    #[serde(rename = "ref")]
    git_ref: String,
    #[serde(default)]
    inputs: Option<Value>,
}

impl ArtifactPath {
    fn ref_input(&self) -> Value {
        json!({
            "owner": self.owner,
            "name": self.repo,
            "artifact_id": self.artifact_id,
        })
    }
}

#[derive(Deserialize)]
struct SecretPath {
    owner: String,
    repo: String,
    secret_name: String,
}

#[derive(Deserialize)]
struct VarPath {
    owner: String,
    repo: String,
    variable_name: String,
}

#[derive(Deserialize)]
struct EnvPath {
    owner: String,
    repo: String,
    environment_name: String,
}

#[derive(Deserialize)]
struct EnvSecretPath {
    owner: String,
    repo: String,
    environment_name: String,
    secret_name: String,
}

#[derive(Deserialize)]
struct EnvVarPath {
    owner: String,
    repo: String,
    environment_name: String,
    variable_name: String,
}

#[derive(Deserialize)]
struct OrgPath {
    org: String,
}

#[derive(Deserialize)]
struct OrgSecretPath {
    org: String,
    secret_name: String,
}

#[derive(Deserialize)]
struct OrgVarPath {
    org: String,
    variable_name: String,
}

#[derive(Deserialize)]
struct OrgSecretRepoPath {
    org: String,
    secret_name: String,
    repository_id: String,
}

#[derive(Deserialize)]
struct OrgVarRepoPath {
    org: String,
    variable_name: String,
    repository_id: String,
}

#[derive(Deserialize)]
struct DeploymentPath {
    owner: String,
    repo: String,
    deployment_id: String,
}

/// `/{owner}/{repo}/actions/caches/{cache_id}`.
#[derive(Deserialize)]
struct CachePath {
    owner: String,
    repo: String,
    cache_id: String,
}

/// `/{owner}/{repo}/actions/runs/{run_id}/attempts/{attempt_number}`.
#[derive(Deserialize)]
struct AttemptPath {
    owner: String,
    repo: String,
    run_id: String,
    attempt_number: i64,
}

/// `GET /actions/caches` query — GitHub's key/ref filters + sort/direction.
#[derive(Deserialize, schemars::JsonSchema)]
struct CachesQuery {
    key: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    sort: Option<String>,
    direction: Option<String>,
    per_page: Option<u32>,
    page: Option<u32>,
}

/// `DELETE /actions/caches` query — key+ref bulk delete.
#[derive(Deserialize, schemars::JsonSchema)]
struct CachesDeleteQuery {
    key: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
}

/// `PUT …/secrets/{name}` body — GitHub sends `{encrypted_value, key_id}`
/// sealed against the `public-key` endpoint; `value` stays accepted for
/// first-party/plaintext clients.
#[derive(Deserialize, schemars::JsonSchema)]
struct SecretPutBody {
    encrypted_value: Option<String>,
    /// Accepted for wire compatibility — one derived keypair per scope means
    /// the id is informational.
    #[allow(dead_code)]
    key_id: Option<String>,
    value: Option<String>,
    /// org-only: 'all' | 'private' | 'selected'.
    visibility: Option<String>,
    /// org-only: repo ids for 'selected' visibility.
    selected_repository_ids: Option<Vec<String>>,
}

/// `POST/PATCH …/variables[/name]` body.
#[derive(Deserialize, schemars::JsonSchema)]
struct VariableBody {
    name: Option<String>,
    value: Option<String>,
    visibility: Option<String>,
    selected_repository_ids: Option<Vec<String>>,
}

/// `PUT …/environments/{name}` body — GitHub's protection-rule shape mapped
/// onto our `protection` object.
#[derive(Deserialize, schemars::JsonSchema)]
struct EnvUpsertBody {
    wait_timer: Option<i64>,
    prevent_self_review: Option<bool>,
    /// `[{type: "User"|"Team", id}]` — ids may be numbers or login strings.
    reviewers: Option<Vec<Value>>,
    /// `{protected_branches, custom_branch_policies, branches?}` — `branches`
    /// is an Oxidean extension carrying the allowlist itself.
    deployment_branch_policy: Option<Value>,
}

pub const ROUTES: &[RouteDef] = &[
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/artifacts",
        tags: &["actions"],
        operation_id: "actions/listArtifacts",
        summary: "List repo artifacts, GitHub shape (repo.actions.listAllArtifacts)",
        procedure: "repo.actions.listAllArtifacts",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<ArtifactsQuery>),
        body: None,
        response: Some(RespSpec::Json(artifacts_list_schema)),
        mount: || get(list_repo_artifacts),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/artifacts",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRunArtifacts",
        summary: "List a run's artifacts, GitHub shape (repo.actions.listArtifacts)",
        procedure: "repo.actions.listArtifacts",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: Some(SchemaGenerator::into_root_schema_for::<ArtifactsQuery>),
        body: None,
        response: Some(RespSpec::Json(artifacts_list_schema)),
        mount: || get(list_run_artifacts),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}",
        tags: &["actions"],
        operation_id: "actions/getArtifact",
        summary: "One artifact's metadata, GitHub shape (repo.actions.getArtifact)",
        procedure: "repo.actions.getArtifact",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "artifact_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(artifact_schema)),
        mount: || get(get_artifact),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}/zip",
        tags: &["actions"],
        operation_id: "actions/downloadArtifact",
        summary: "Stream the artifact archive as zip (repo.actions.getArtifact)",
        procedure: "repo.actions.getArtifact",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "artifact_id"],
        query: None,
        body: None,
        response: None,
        mount: || get(download_artifact_zip),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/artifacts/{artifact_id}",
        tags: &["actions"],
        operation_id: "actions/deleteArtifact",
        summary: "Delete an artifact (repo.actions.deleteArtifact)",
        procedure: "repo.actions.deleteArtifact",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "artifact_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_artifact),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRuns",
        summary: "List workflow runs, GitHub shape (repo.actions.listRuns)",
        procedure: "repo.actions.listRuns",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<RunsQuery>),
        body: None,
        response: Some(RespSpec::Json(runs_list_schema)),
        mount: || get(list_runs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}",
        tags: &["actions"],
        operation_id: "actions/getWorkflowRun",
        summary: "One workflow run, GitHub shape (repo.actions.getRun)",
        procedure: "repo.actions.getRun",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(run_schema)),
        mount: || get(get_run),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/jobs",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRunJobs",
        summary: "List a run's jobs, GitHub shape (repo.actions.getRun)",
        procedure: "repo.actions.getRun",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(jobs_list_schema)),
        mount: || get(list_run_jobs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/jobs/{job_id}",
        tags: &["actions"],
        operation_id: "actions/getJobForWorkflowRun",
        summary: "One job, GitHub shape (repo.actions.getJob)",
        procedure: "repo.actions.getJob",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "job_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(job_schema)),
        mount: || get(get_job),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/rerun",
        tags: &["actions"],
        operation_id: "actions/reRunWorkflow",
        summary: "Re-run a workflow run (repo.actions.rerunRun)",
        procedure: "repo.actions.rerunRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(rerun_run),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/rerun-failed",
        tags: &["actions"],
        operation_id: "actions/reRunWorkflowFailed",
        summary: "Re-run failed jobs only (repo.actions.rerunRun failed_only)",
        procedure: "repo.actions.rerunRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(rerun_failed),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/jobs/{job_id}/rerun",
        tags: &["actions"],
        operation_id: "actions/reRunJobForWorkflowRun",
        summary: "Re-run one job (repo.actions.rerunRun job_id)",
        procedure: "repo.actions.rerunRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "job_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(rerun_job),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/cancel",
        tags: &["actions"],
        operation_id: "actions/cancelWorkflowRun",
        summary: "Cancel a workflow run (repo.actions.cancelRun)",
        procedure: "repo.actions.cancelRun",
        ok: StatusCode::ACCEPTED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(cancel_run),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/approve",
        tags: &["actions"],
        operation_id: "actions/approveWorkflowRun",
        summary: "Approve a fork-held run (repo.actions.approveRun)",
        procedure: "repo.actions.approveRun",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || post(approve_run),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}",
        tags: &["actions"],
        operation_id: "actions/deleteWorkflowRun",
        summary: "Delete a workflow run (repo.actions.deleteRun)",
        procedure: "repo.actions.deleteRun",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_run),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/jobs/{job_id}/logs",
        tags: &["actions"],
        operation_id: "actions/downloadJobLogsForWorkflowRun",
        summary: "Stream a job's log (repo.actions.getJobLog)",
        procedure: "repo.actions.getJobLog",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "job_id"],
        query: None,
        body: None,
        response: None,
        mount: || get(crate::routes::action_downloads::download_job_log),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/logs",
        tags: &["actions"],
        operation_id: "actions/downloadWorkflowRunLogs",
        summary: "Download the run's logs zip (repo.actions.getJobLog)",
        procedure: "repo.actions.getJobLog",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || get(crate::routes::action_downloads::download_run_logs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/workflows",
        tags: &["actions"],
        operation_id: "actions/listRepoWorkflows",
        summary: "List repo workflows, GitHub shape (repo.actions.listWorkflows)",
        procedure: "repo.actions.listWorkflows",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(workflows_list_schema)),
        mount: || get(list_workflows),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}",
        tags: &["actions"],
        operation_id: "actions/getWorkflow",
        summary: "One workflow by id/filename/name (repo.actions.listWorkflows)",
        procedure: "repo.actions.listWorkflows",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(workflow_schema)),
        mount: || get(get_workflow),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/runs",
        tags: &["actions"],
        operation_id: "actions/listWorkflowRunsForWorkflow",
        summary: "List one workflow's runs (repo.actions.listRuns)",
        procedure: "repo.actions.listRuns",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<RunsQuery>),
        body: None,
        response: Some(RespSpec::Json(runs_list_schema)),
        mount: || get(list_workflow_runs),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches",
        tags: &["actions"],
        operation_id: "actions/createWorkflowDispatch",
        summary: "workflow_dispatch a workflow (repo.actions.dispatchWorkflow)",
        procedure: "repo.actions.dispatchWorkflow",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(dispatch_body_schema)),
        response: None,
        mount: || post(dispatch_workflow),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/enable",
        tags: &["actions"],
        operation_id: "actions/enableWorkflow",
        summary: "Enable a workflow (repo.actions.setWorkflowEnabled)",
        procedure: "repo.actions.setWorkflowEnabled",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || put(enable_workflow),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/actions/workflows/{workflow_id}/disable",
        tags: &["actions"],
        operation_id: "actions/disableWorkflow",
        summary: "Disable a workflow (repo.actions.setWorkflowEnabled)",
        procedure: "repo.actions.setWorkflowEnabled",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: None,
        mount: || put(disable_workflow),
    },
    // ---- secrets / variables / environments — the `gh secret|variable`
    // and environment-admin surface ----
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/secrets",
        tags: &["actions"],
        operation_id: "actions/listRepoSecrets",
        summary: "List repo secrets, GitHub shape (repo.actions.secrets.list)",
        procedure: "repo.actions.secrets.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(secrets_list_schema)),
        mount: || get(list_repo_secrets),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/secrets/public-key",
        tags: &["actions"],
        operation_id: "actions/getRepoPublicKey",
        summary: "Repo sealed-box public key (repo.actions.secrets.publicKey)",
        procedure: "repo.actions.secrets.publicKey",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(public_key_schema)),
        mount: || get(repo_public_key),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/getRepoSecret",
        summary: "One repo secret's metadata (repo.actions.secrets.list)",
        procedure: "repo.actions.secrets.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "secret_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(secret_schema)),
        mount: || get(get_repo_secret),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/actions/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/createOrUpdateRepoSecret",
        summary: "Create/update a repo secret (repo.actions.secrets.put)",
        procedure: "repo.actions.secrets.put",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "secret_name"],
        query: None,
        body: Some(BodySpec::Json(secret_put_body_schema)),
        response: None,
        mount: || put(put_repo_secret),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/deleteRepoSecret",
        summary: "Delete a repo secret (repo.actions.secrets.delete)",
        procedure: "repo.actions.secrets.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "secret_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_repo_secret),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/secrets",
        tags: &["actions"],
        operation_id: "actions/listEnvironmentSecrets",
        summary: "List environment secrets (repo.actions.environments.secretsList)",
        procedure: "repo.actions.environments.secretsList",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "environment"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(secrets_list_schema)),
        mount: || get(list_env_secrets),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/secrets/public-key",
        tags: &["actions"],
        operation_id: "actions/getEnvironmentPublicKey",
        summary: "Environment sealed-box public key (repo.actions.environments.secretsPublicKey)",
        procedure: "repo.actions.environments.secretsPublicKey",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "environment"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(public_key_schema)),
        mount: || get(env_public_key),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/getEnvironmentSecret",
        summary: "One environment secret's metadata (repo.actions.environments.secretsList)",
        procedure: "repo.actions.environments.secretsList",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "environment", "secret_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(secret_schema)),
        mount: || get(get_env_secret),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/createOrUpdateEnvironmentSecret",
        summary: "Create/update an environment secret (repo.actions.environments.secretsPut)",
        procedure: "repo.actions.environments.secretsPut",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "environment", "secret_name"],
        query: None,
        body: Some(BodySpec::Json(secret_put_body_schema)),
        response: None,
        mount: || put(put_env_secret),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/deleteEnvironmentSecret",
        summary: "Delete an environment secret (repo.actions.environments.secretsDelete)",
        procedure: "repo.actions.environments.secretsDelete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "environment", "secret_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_env_secret),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/actions/secrets",
        tags: &["actions"],
        operation_id: "actions/listOrgSecrets",
        summary: "List org secrets (org.actions.secrets.list)",
        procedure: "org.actions.secrets.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(secrets_list_schema)),
        mount: || get(list_org_secrets),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/actions/secrets/public-key",
        tags: &["actions"],
        operation_id: "actions/getOrgPublicKey",
        summary: "Org sealed-box public key (org.actions.secrets.publicKey)",
        procedure: "org.actions.secrets.publicKey",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(public_key_schema)),
        mount: || get(org_public_key),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/actions/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/getOrgSecret",
        summary: "One org secret's metadata (org.actions.secrets.list)",
        procedure: "org.actions.secrets.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug", "secret_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(org_secret_schema)),
        mount: || get(get_org_secret),
    },
    RouteDef {
        method: "PUT",
        path: "/orgs/{org}/actions/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/createOrUpdateOrgSecret",
        summary: "Create/update an org secret (org.actions.secrets.put)",
        procedure: "org.actions.secrets.put",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "secret_name"],
        query: None,
        body: Some(BodySpec::Json(secret_put_body_schema)),
        response: None,
        mount: || put(put_org_secret),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{org}/actions/secrets/{secret_name}",
        tags: &["actions"],
        operation_id: "actions/deleteOrgSecret",
        summary: "Delete an org secret (org.actions.secrets.delete)",
        procedure: "org.actions.secrets.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "secret_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_org_secret),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/actions/secrets/{secret_name}/selected-repositories",
        tags: &["actions"],
        operation_id: "actions/listSelectedReposForOrgSecret",
        summary: "Repos a 'selected' org secret reaches (org.actions.secrets.listRepos)",
        procedure: "org.actions.secrets.listRepos",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug", "secret_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(selected_repos_schema)),
        mount: || get(org_secret_selected_repos),
    },
    RouteDef {
        method: "PUT",
        path: "/orgs/{org}/actions/secrets/{secret_name}/selected-repositories",
        tags: &["actions"],
        operation_id: "actions/setSelectedReposForOrgSecret",
        summary: "Replace an org secret's selected repo set (org.actions.secrets.setRepos)",
        procedure: "org.actions.secrets.setRepos",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "secret_name"],
        query: None,
        body: Some(BodySpec::Json(selected_repos_body_schema)),
        response: None,
        mount: || put(org_secret_set_selected_repos),
    },
    RouteDef {
        method: "PUT",
        path: "/orgs/{org}/actions/secrets/{secret_name}/selected-repositories/{repository_id}",
        tags: &["actions"],
        operation_id: "actions/addSelectedRepoToOrgSecret",
        summary: "Add a repo to an org secret's selected set (org.actions.secrets.setRepos)",
        procedure: "org.actions.secrets.setRepos",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "secret_name", "repository_id"],
        query: None,
        body: None,
        response: None,
        mount: || put(org_secret_add_selected_repo),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{org}/actions/secrets/{secret_name}/selected-repositories/{repository_id}",
        tags: &["actions"],
        operation_id: "actions/removeSelectedRepoFromOrgSecret",
        summary: "Remove a repo from an org secret's selected set (org.actions.secrets.setRepos)",
        procedure: "org.actions.secrets.setRepos",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "secret_name", "repository_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(org_secret_remove_selected_repo),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/variables",
        tags: &["actions"],
        operation_id: "actions/listRepoVariables",
        summary: "List repo variables (repo.actions.variables.list)",
        procedure: "repo.actions.variables.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(variables_list_schema)),
        mount: || get(list_repo_variables),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/variables",
        tags: &["actions"],
        operation_id: "actions/createRepoVariable",
        summary: "Create a repo variable — 409 on duplicate (repo.actions.variables.put)",
        procedure: "repo.actions.variables.put",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(variable_body_schema)),
        response: None,
        mount: || post(create_repo_variable),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/getRepoVariable",
        summary: "One repo variable (repo.actions.variables.list)",
        procedure: "repo.actions.variables.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "variable_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(variable_schema)),
        mount: || get(get_repo_variable),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/actions/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/updateRepoVariable",
        summary: "Update a repo variable (repo.actions.variables.put)",
        procedure: "repo.actions.variables.put",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "variable_name"],
        query: None,
        body: Some(BodySpec::Json(variable_body_schema)),
        response: None,
        mount: || patch(update_repo_variable),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/deleteRepoVariable",
        summary: "Delete a repo variable (repo.actions.variables.delete)",
        procedure: "repo.actions.variables.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "variable_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_repo_variable),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/variables",
        tags: &["actions"],
        operation_id: "actions/listEnvironmentVariables",
        summary: "List environment variables (repo.actions.environments.variablesList)",
        procedure: "repo.actions.environments.variablesList",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "environment"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(variables_list_schema)),
        mount: || get(list_env_variables),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/variables",
        tags: &["actions"],
        operation_id: "actions/createEnvironmentVariable",
        summary: "Create an environment variable (repo.actions.environments.variablesPut)",
        procedure: "repo.actions.environments.variablesPut",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "environment"],
        query: None,
        body: Some(BodySpec::Json(variable_body_schema)),
        response: None,
        mount: || post(create_env_variable),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/getEnvironmentVariable",
        summary: "One environment variable (repo.actions.environments.variablesList)",
        procedure: "repo.actions.environments.variablesList",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "environment", "variable_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(variable_schema)),
        mount: || get(get_env_variable),
    },
    RouteDef {
        method: "PATCH",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/updateEnvironmentVariable",
        summary: "Update an environment variable (repo.actions.environments.variablesPut)",
        procedure: "repo.actions.environments.variablesPut",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "environment", "variable_name"],
        query: None,
        body: Some(BodySpec::Json(variable_body_schema)),
        response: None,
        mount: || patch(update_env_variable),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/environments/{environment_name}/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/deleteEnvironmentVariable",
        summary: "Delete an environment variable (repo.actions.environments.variablesDelete)",
        procedure: "repo.actions.environments.variablesDelete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "environment", "variable_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_env_variable),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/actions/variables",
        tags: &["actions"],
        operation_id: "actions/listOrgVariables",
        summary: "List org variables (org.actions.variables.list)",
        procedure: "org.actions.variables.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(variables_list_schema)),
        mount: || get(list_org_variables),
    },
    RouteDef {
        method: "POST",
        path: "/orgs/{org}/actions/variables",
        tags: &["actions"],
        operation_id: "actions/createOrgVariable",
        summary: "Create an org variable (org.actions.variables.put)",
        procedure: "org.actions.variables.put",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["slug"],
        query: None,
        body: Some(BodySpec::Json(variable_body_schema)),
        response: None,
        mount: || post(create_org_variable),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/actions/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/getOrgVariable",
        summary: "One org variable (org.actions.variables.list)",
        procedure: "org.actions.variables.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug", "variable_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(org_variable_schema)),
        mount: || get(get_org_variable),
    },
    RouteDef {
        method: "PATCH",
        path: "/orgs/{org}/actions/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/updateOrgVariable",
        summary: "Update an org variable (org.actions.variables.put)",
        procedure: "org.actions.variables.put",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "variable_name"],
        query: None,
        body: Some(BodySpec::Json(variable_body_schema)),
        response: None,
        mount: || patch(update_org_variable),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{org}/actions/variables/{variable_name}",
        tags: &["actions"],
        operation_id: "actions/deleteOrgVariable",
        summary: "Delete an org variable (org.actions.variables.delete)",
        procedure: "org.actions.variables.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "variable_name"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_org_variable),
    },
    RouteDef {
        method: "GET",
        path: "/orgs/{org}/actions/variables/{variable_name}/selected-repositories",
        tags: &["actions"],
        operation_id: "actions/listSelectedReposForOrgVariable",
        summary: "Repos a 'selected' org variable reaches (org.actions.variables.listRepos)",
        procedure: "org.actions.variables.listRepos",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["slug", "variable_name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(selected_repos_schema)),
        mount: || get(org_variable_selected_repos),
    },
    RouteDef {
        method: "PUT",
        path: "/orgs/{org}/actions/variables/{variable_name}/selected-repositories",
        tags: &["actions"],
        operation_id: "actions/setSelectedReposForOrgVariable",
        summary: "Replace an org variable's selected repo set (org.actions.variables.setRepos)",
        procedure: "org.actions.variables.setRepos",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "variable_name"],
        query: None,
        body: Some(BodySpec::Json(selected_repos_body_schema)),
        response: None,
        mount: || put(org_variable_set_selected_repos),
    },
    RouteDef {
        method: "PUT",
        path: "/orgs/{org}/actions/variables/{variable_name}/selected-repositories/{repository_id}",
        tags: &["actions"],
        operation_id: "actions/addSelectedRepoToOrgVariable",
        summary: "Add a repo to an org variable's selected set (org.actions.variables.setRepos)",
        procedure: "org.actions.variables.setRepos",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "variable_name", "repository_id"],
        query: None,
        body: None,
        response: None,
        mount: || put(org_variable_add_selected_repo),
    },
    RouteDef {
        method: "DELETE",
        path: "/orgs/{org}/actions/variables/{variable_name}/selected-repositories/{repository_id}",
        tags: &["actions"],
        operation_id: "actions/removeSelectedRepoFromOrgVariable",
        summary:
            "Remove a repo from an org variable's selected set (org.actions.variables.setRepos)",
        procedure: "org.actions.variables.setRepos",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["slug", "variable_name", "repository_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(org_variable_remove_selected_repo),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/environments",
        tags: &["actions"],
        operation_id: "actions/listEnvironments",
        summary: "List environments (repo.actions.environments.list)",
        procedure: "repo.actions.environments.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(environments_list_schema)),
        mount: || get(list_environments),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/environments/{environment_name}",
        tags: &["actions"],
        operation_id: "actions/getEnvironment",
        summary: "One environment (repo.actions.environments.get)",
        procedure: "repo.actions.environments.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "environment"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(environment_schema)),
        mount: || get(get_environment),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/environments/{environment_name}",
        tags: &["actions"],
        operation_id: "actions/createOrUpdateEnvironment",
        summary: "Create/update an environment (repo.actions.environments.upsert)",
        procedure: "repo.actions.environments.upsert",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "environment"],
        query: None,
        body: Some(BodySpec::Json(env_upsert_body_schema)),
        response: Some(RespSpec::Json(environment_schema)),
        mount: || put(put_environment),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/environments/{environment_name}",
        tags: &["actions"],
        operation_id: "actions/deleteEnvironment",
        summary: "Delete an environment (repo.actions.environments.delete)",
        procedure: "repo.actions.environments.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "environment"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_environment),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/deployments",
        tags: &["actions"],
        operation_id: "repos/listDeployments",
        summary: "List deployments, GitHub shape (deployment.list)",
        procedure: "deployment.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<DeploymentsQuery>),
        body: None,
        response: Some(RespSpec::Json(deployments_list_schema)),
        mount: || get(list_deployments),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/deployments",
        tags: &["actions"],
        operation_id: "repos/createDeployment",
        summary: "Create a deployment, GitHub shape (deployment.create)",
        procedure: "deployment.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(deployment_create_schema)),
        response: Some(RespSpec::Json(deployment_schema)),
        mount: || post(create_deployment),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/deployments/{deployment_id}",
        tags: &["actions"],
        operation_id: "repos/getDeployment",
        summary: "One deployment, GitHub shape (deployment.get)",
        procedure: "deployment.get",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(deployment_schema)),
        mount: || get(get_deployment),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/deployments/{deployment_id}",
        tags: &["actions"],
        operation_id: "repos/deleteDeployment",
        summary: "Delete a deployment (deployment.delete)",
        procedure: "deployment.delete",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_deployment),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/deployments/{deployment_id}/statuses",
        tags: &["actions"],
        operation_id: "repos/listDeploymentStatuses",
        summary: "List a deployment's statuses, GitHub shape (deployment.status.list)",
        procedure: "deployment.status.list",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "deployment_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(deployment_statuses_schema)),
        mount: || get(list_deployment_statuses),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/deployments/{deployment_id}/statuses",
        tags: &["actions"],
        operation_id: "repos/createDeploymentStatus",
        summary: "Create a deployment status, GitHub shape (deployment.status.create)",
        procedure: "deployment.status.create",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name", "deployment_id"],
        query: None,
        body: Some(BodySpec::Json(deployment_status_body_schema)),
        response: Some(RespSpec::Json(deployment_status_schema)),
        mount: || post(create_deployment_status),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/dispatches",
        tags: &["actions"],
        operation_id: "repos/createDispatchEvent",
        summary: "repository_dispatch event (repo.actions.dispatchRepository)",
        procedure: "repo.actions.dispatchRepository",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(repo_dispatch_schema)),
        response: None,
        mount: || post(repository_dispatch),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/permissions",
        tags: &["actions"],
        operation_id: "actions/getActionsPermissionsRepository",
        summary: "Repo Actions permissions {enabled, allowed_actions} (repo.actions.getEnabled)",
        procedure: "repo.actions.getEnabled",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(actions_permissions_schema)),
        mount: || get(get_actions_permissions),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/actions/permissions",
        tags: &["actions"],
        operation_id: "actions/setActionsPermissionsRepository",
        summary: "Set repo Actions permissions (repo.actions.setEnabled)",
        procedure: "repo.actions.setEnabled",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(actions_permissions_put_schema)),
        response: None,
        mount: || put(set_actions_permissions),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/permissions/workflow",
        tags: &["actions"],
        operation_id: "actions/getWorkflowPermissionsRepository",
        summary: "Default token permissions + PR-approve flag (getDefaultPermissions + getPullRequestTarget)",
        procedure: "repo.actions.getDefaultPermissions",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(workflow_permissions_schema)),
        mount: || get(get_workflow_permissions),
    },
    RouteDef {
        method: "PUT",
        path: "/repos/{owner}/{repo}/actions/permissions/workflow",
        tags: &["actions"],
        operation_id: "actions/setWorkflowPermissionsRepository",
        summary: "Set default token permissions (setDefaultPermissions / setPullRequestTarget)",
        procedure: "repo.actions.setDefaultPermissions",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: Some(BodySpec::Json(workflow_permissions_schema)),
        response: None,
        mount: || put(set_workflow_permissions),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments",
        tags: &["actions"],
        operation_id: "actions/getPendingDeployments",
        summary: "Environments with approval-held jobs for a run (repo.actions.getRun)",
        procedure: "repo.actions.getRun",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(pending_deployments_schema)),
        mount: || get(list_pending_deployments),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/pending_deployments",
        tags: &["actions"],
        operation_id: "actions/reviewPendingDeployments",
        summary: "Approve/reject environment-held jobs (repo.actions.reviewDeployment)",
        procedure: "repo.actions.reviewDeployment",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: Some(BodySpec::Json(pending_deployments_body_schema)),
        response: Some(RespSpec::Json(pending_deployments_schema)),
        mount: || post(review_pending_deployments),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/caches",
        tags: &["actions"],
        operation_id: "actions/getActionsCacheList",
        summary: "List repo cache entries, GitHub shape (repo.actions.listCaches)",
        procedure: "repo.actions.listCaches",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<CachesQuery>),
        body: None,
        response: Some(RespSpec::Json(caches_list_schema)),
        mount: || get(list_gh_caches),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/caches",
        tags: &["actions"],
        operation_id: "actions/deleteActionsCacheByKey",
        summary: "Delete caches matching key/ref (repo.actions.listCaches + deleteCache)",
        procedure: "repo.actions.deleteCache",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: Some(SchemaGenerator::into_root_schema_for::<CachesDeleteQuery>),
        body: None,
        response: None,
        mount: || delete(delete_gh_caches_by_key),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/caches/{cache_id}",
        tags: &["actions"],
        operation_id: "actions/deleteActionsCacheById",
        summary: "Delete one cache entry (repo.actions.deleteCache)",
        procedure: "repo.actions.deleteCache",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "cache_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_gh_cache),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/attempts/{attempt_number}",
        tags: &["actions"],
        operation_id: "actions/getWorkflowRunAttempt",
        summary: "Get a run attempt — reruns replace jobs in place, so only the current attempt answers (repo.actions.getRun)",
        procedure: "repo.actions.getRun",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(run_schema)),
        mount: || get(get_run_attempt),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/attempts/{attempt_number}/jobs",
        tags: &["actions"],
        operation_id: "actions/listJobsForWorkflowRunAttempt",
        summary: "Jobs for a run attempt (repo.actions.getRun)",
        procedure: "repo.actions.getRun",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(jobs_list_schema)),
        mount: || get(list_attempt_jobs),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/attempts/{attempt_number}/logs",
        tags: &["actions"],
        operation_id: "actions/downloadWorkflowRunAttemptLogs",
        summary: "Log archive for a run attempt (repo.actions.getJobLog)",
        procedure: "repo.actions.getJobLog",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || get(download_attempt_logs),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/runs/{run_id}/logs",
        tags: &["actions"],
        operation_id: "actions/deleteWorkflowRunLogs",
        summary: "Delete a run's stored logs (repo.actions.deleteRunLogs)",
        procedure: "repo.actions.deleteRunLogs",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "run_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_run_logs_rest),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/cache/usage",
        tags: &["actions"],
        operation_id: "actions/getActionsCacheUsage",
        summary: "Repo cache usage rollup (repo.actions.listCaches totals)",
        procedure: "repo.actions.listCaches",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(cache_usage_schema)),
        mount: || get(get_cache_usage),
    },
    // --- self-hosted runners (repo scope) ---
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runners",
        tags: &["actions"],
        operation_id: "actions/listSelfHostedRunnersForRepo",
        summary: "List self-hosted runners visible to the repo (repo.actions.listRunners)",
        procedure: "repo.actions.listRunners",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(runners_list_schema)),
        mount: || get(list_runners_rest),
    },
    RouteDef {
        method: "POST",
        path: "/repos/{owner}/{repo}/actions/runners/registration-token",
        tags: &["actions"],
        operation_id: "actions/createRunnerRegistrationToken",
        summary: "Mint a repo-scoped registration token (repo.actions.createRegistrationToken)",
        procedure: "repo.actions.createRegistrationToken",
        ok: StatusCode::CREATED,
        anonymous: false,
        path_fields: &["owner", "name"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(registration_token_schema)),
        mount: || post(create_runner_registration_token),
    },
    RouteDef {
        method: "GET",
        path: "/repos/{owner}/{repo}/actions/runners/{runner_id}",
        tags: &["actions"],
        operation_id: "actions/getSelfHostedRunnerForRepo",
        summary: "One runner, GitHub shape (repo.actions.getRunner)",
        procedure: "repo.actions.getRunner",
        ok: StatusCode::OK,
        anonymous: false,
        path_fields: &["owner", "name", "runner_id"],
        query: None,
        body: None,
        response: Some(RespSpec::Json(runner_schema)),
        mount: || get(get_runner_rest),
    },
    RouteDef {
        method: "DELETE",
        path: "/repos/{owner}/{repo}/actions/runners/{runner_id}",
        tags: &["actions"],
        operation_id: "actions/deleteSelfHostedRunnerFromRepo",
        summary: "Remove a repo-bound runner (repo.actions.deleteRunner)",
        procedure: "repo.actions.deleteRunner",
        ok: StatusCode::NO_CONTENT,
        anonymous: false,
        path_fields: &["owner", "name", "runner_id"],
        query: None,
        body: None,
        response: None,
        mount: || delete(delete_runner_rest),
    },
];

fn artifacts_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "artifacts": {"type": "array", "items": artifact_schema_value()},
        },
        "required": ["total_count", "artifacts"],
    })
}

fn artifact_schema(_gen: &mut SchemaGenerator) -> Value {
    artifact_schema_value()
}

fn artifact_schema_value() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {},
            "name": {"type": "string"},
            "size_in_bytes": {"type": "integer"},
            "url": {"type": "string"},
            "archive_download_url": {"type": "string"},
            "expired": {"type": "boolean"},
            "created_at": {"type": "string"},
            "expires_at": {"type": ["string", "null"]},
            "digest": {"type": ["string", "null"]},
            "workflow_run": {"type": "object"},
        },
    })
}

/// SQLite artifact rows store `%Y-%m-%d %H:%M:%S` — emit GitHub's RFC3339.
fn iso(v: &Value) -> Value {
    match v.as_str() {
        Some(s) if s.len() == 19 && s.as_bytes()[10] == b' ' => {
            json!(format!("{}T{}Z", &s[..10], &s[11..]))
        }
        _ => v.clone(),
    }
}

/// `ActionArtifactPublic` JSON → the GitHub REST artifact object.
fn gh_artifact(origin: &str, owner: &str, repo: &str, a: &Value) -> Value {
    let id = a["id"].as_str().unwrap_or_default();
    let run_id = a["run_id"].as_str().unwrap_or_default();
    let id_json = match id.parse::<i64>() {
        Ok(n) => json!(n),
        Err(_) => json!(id),
    };
    let run_json = match run_id.parse::<i64>() {
        Ok(n) => json!(n),
        Err(_) => json!(run_id),
    };
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}/actions/artifacts/{id}");
    let expires = a["expires_at"].as_str();
    let expired = expires
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
        .is_some_and(|e| e < chrono::Utc::now());
    let digest = a["digest_sha256"].as_str().unwrap_or_default();
    json!({
        "id": id_json,
        "node_id": "",
        "name": a["name"].as_str().unwrap_or_default(),
        "size_in_bytes": a["size_bytes"].as_i64().unwrap_or_default(),
        "url": base,
        "archive_download_url": format!("{base}/zip"),
        "expired": expired,
        "created_at": iso(&a["created_at"]),
        "updated_at": iso(&a["created_at"]),
        "expires_at": expires.map(|e| json!(e)).unwrap_or(Value::Null),
        "digest": if digest.is_empty() {
            Value::Null
        } else {
            json!(format!("sha256:{digest}"))
        },
        "workflow_run": { "id": run_json },
    })
}

fn page_slice<'a>(q: &ArtifactsQuery, items: &'a [Value]) -> &'a [Value] {
    let per_page = q.per_page.unwrap_or(30).clamp(1, 100) as usize;
    let page = q.page.unwrap_or(1).max(1) as usize;
    let start = ((page - 1) * per_page).min(items.len());
    &items[start..(start + per_page).min(items.len())]
}

/// Shared reshape: RPC `artifacts` array → `{total_count, artifacts:[gh]}`.
fn gh_list(owner: &str, repo: &str, q: &ArtifactsQuery, data: Value) -> Value {
    let origin = crate::public_origin::resolve_public_origin();
    let mut items = data["artifacts"].as_array().cloned().unwrap_or_default();
    if let Some(name) = q.name.as_deref() {
        items.retain(|a| a["name"].as_str() == Some(name));
    }
    let total = items.len();
    let page: Vec<Value> = page_slice(q, &items)
        .iter()
        .map(|a| gh_artifact(&origin, owner, repo, a))
        .collect();
    json!({"total_count": total, "artifacts": page})
}

/// GET /repos/{o}/{r}/actions/artifacts — repo-wide, GitHub shape.
async fn list_repo_artifacts(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    Query(q): Query<ArtifactsQuery>,
    headers: HeaderMap,
) -> Response {
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    call_map(
        &state,
        &headers,
        "repo.actions.listAllArtifacts",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        move |data| gh_list(&owner, &repo, &q, data),
    )
    .await
}

/// GET /repos/{o}/{r}/actions/runs/{run_id}/artifacts — the cross-run list
/// `download-artifact` paginates.
async fn list_run_artifacts(
    State(state): State<AppState>,
    Path(p): Path<RunArtifactsPath>,
    Query(q): Query<ArtifactsQuery>,
    headers: HeaderMap,
) -> Response {
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["run_id"] = json!(p.run_id);
    call_map(
        &state,
        &headers,
        "repo.actions.listArtifacts",
        input,
        StatusCode::OK,
        move |data| gh_list(&owner, &repo, &q, data),
    )
    .await
}

/// GET /repos/{o}/{r}/actions/artifacts/{id} — bare GitHub artifact object.
async fn get_artifact(
    State(state): State<AppState>,
    Path(p): Path<ArtifactPath>,
    headers: HeaderMap,
) -> Response {
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    call_map(
        &state,
        &headers,
        "repo.actions.getArtifact",
        p.ref_input(),
        StatusCode::OK,
        move |data| {
            gh_artifact(
                &crate::public_origin::resolve_public_origin(),
                &owner,
                &repo,
                &data["artifact"],
            )
        },
    )
    .await
}

/// GET /repos/{o}/{r}/actions/artifacts/{id}/zip — the download Octokit
/// follows. Stored zip payloads pass through; builtin tar.gz repacks.
async fn download_artifact_zip(
    State(state): State<AppState>,
    Path(p): Path<ArtifactPath>,
    headers: HeaderMap,
) -> Response {
    // Auth + existence through the real RPC seam, then the storage path.
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(&mut ctx, "repo.actions.getArtifact", p.ref_input()).await;
    if let oxidean_core::RpcResponse::Err { error, .. } = resp {
        return (status_for_error(&error), Json(error)).into_response();
    }
    let row = match state.db.find_action_artifact_by_id(&p.artifact_id).await {
        Ok(Some(r)) => r,
        _ => return err_json(StatusCode::NOT_FOUND, "Artifact not found or expired"),
    };
    // GitHub 410s downloads once `expires_at` passes, even before the backend
    // physically reaps the row.
    let expired = row
        .expires_at
        .as_deref()
        .and_then(|e| chrono::DateTime::parse_from_rfc3339(e).ok())
        .is_some_and(|e| e < chrono::Utc::now());
    if expired {
        return err_json(StatusCode::GONE, "Artifact expired");
    }
    let path = state.actions_artifacts_dir.join(&row.storage_path);
    let bytes = match tokio::fs::read(&path).await {
        Ok(b) => b,
        Err(_) => return err_json(StatusCode::NOT_FOUND, "Artifact not found or expired"),
    };
    let body = match targz_to_zip(&bytes, &row.mime_type) {
        Ok(b) => b,
        Err(_) => return err_json(StatusCode::INTERNAL_SERVER_ERROR, "archive build failed"),
    };
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = StatusCode::OK;
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/zip"),
    );
    if let Ok(hv) = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}.zip\"",
        sanitize_filename(&row.name)
    )) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, hv);
    }
    res
}

/// DELETE /repos/{o}/{r}/actions/artifacts/{id} — GitHub returns bare 204.
async fn delete_artifact(
    State(state): State<AppState>,
    Path(p): Path<ArtifactPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    match dispatch(&mut ctx, "repo.actions.deleteArtifact", p.ref_input()).await {
        oxidean_core::RpcResponse::Ok { .. } => StatusCode::NO_CONTENT.into_response(),
        oxidean_core::RpcResponse::Err { error, .. } => {
            (status_for_error(&error), Json(error)).into_response()
        }
    }
}

fn err_json(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({"message": message}))).into_response()
}

/// Repo the stored blob to zip. v4 Twirp uploads are already zip;
/// builtin runner uploads are tar.gz (repack so `unzip` clients work).
fn targz_to_zip(bytes: &[u8], mime_type: &str) -> std::io::Result<Vec<u8>> {
    let is_gzip = mime_type == "application/gzip" || bytes.starts_with(&[0x1f, 0x8b]);
    if !is_gzip {
        return Ok(bytes.to_vec());
    }
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut out);
        for entry in ar.entries()? {
            let mut e = entry?;
            if !e.header().entry_type().is_file() {
                continue;
            }
            let name = e.path()?.to_string_lossy().replace('\\', "/");
            // Zip-slip guard — archive names are runner-generated, but keep
            // the repack safe against absolute/parent paths regardless.
            if name.starts_with('/') || name.split('/').any(|seg| seg == "..") {
                continue;
            }
            let mut opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            if let Ok(mode) = e.header().mode() {
                opts = opts.unix_permissions(mode);
            }
            w.start_file(&name, opts)?;
            std::io::copy(&mut e, &mut w)?;
        }
        w.finish()?;
    }
    Ok(out.into_inner())
}

// --- runs / jobs / workflows — the `gh run|workflow` surface ---------------

/// Stored status → GitHub's `(status, conclusion)` pair. Non-terminal rows
/// keep a null conclusion; `waiting_approval` reads as GitHub's `waiting`.
fn status_conclusion(status: &str) -> (Value, Value) {
    match status {
        "queued" => (json!("queued"), Value::Null),
        "in_progress" => (json!("in_progress"), Value::Null),
        "waiting" | "waiting_approval" => (json!("waiting"), Value::Null),
        s => (json!("completed"), json!(s)),
    }
}

/// `refs/heads/main` → `main`; other ref forms pass through.
fn short_branch(head_ref: &str) -> &str {
    head_ref.strip_prefix("refs/heads/").unwrap_or(head_ref)
}

fn num_or_str(s: &str) -> Value {
    s.parse::<i64>().map_or_else(|_| json!(s), |n| json!(n))
}

/// `ActionRunPublic` JSON → the GitHub workflow-run object.
fn gh_run(origin: &str, owner: &str, repo: &str, r: &Value) -> Value {
    let id = r["id"].as_str().unwrap_or_default();
    let (status, conclusion) = status_conclusion(r["status"].as_str().unwrap_or_default());
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    let html_repo = format!("{origin}/{owner}/{repo}");
    json!({
        "id": num_or_str(id),
        "name": r["workflow_name"].as_str().unwrap_or_default(),
        "node_id": "",
        "head_branch": short_branch(r["head_ref"].as_str().unwrap_or_default()),
        "head_sha": r["head_sha"].as_str().unwrap_or_default(),
        "path": r["workflow_path"].as_str().unwrap_or_default(),
        "run_number": r["run_number"].as_i64().unwrap_or_default(),
        "run_attempt": r["attempt"].as_i64().unwrap_or(1),
        "event": r["event"].as_str().unwrap_or_default(),
        "status": status,
        "conclusion": conclusion,
        "display_title": r["title"].as_str().unwrap_or_default(),
        "url": format!("{base}/actions/runs/{id}"),
        "html_url": format!("{html_repo}/actions/runs/{id}"),
        "created_at": iso(&r["created_at"]),
        "updated_at": iso(&r["updated_at"]),
        "actor": r["actor"].as_str().map(|a| json!({"login": a})),
        "triggering_actor": r["actor"].as_str().map(|a| json!({"login": a})),
        "repository_id": num_or_str(r["repository_id"].as_str().unwrap_or_default()),
    })
}

/// `ActionJobPublic` JSON → the GitHub job object.
fn gh_job(origin: &str, owner: &str, repo: &str, j: &Value, run_name: Option<&str>) -> Value {
    let id = j["id"].as_str().unwrap_or_default();
    let run_id = j["run_id"].as_str().unwrap_or_default();
    let (status, conclusion) = status_conclusion(j["status"].as_str().unwrap_or_default());
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": num_or_str(id),
        "run_id": num_or_str(run_id),
        "name": j["name"].as_str().unwrap_or_default(),
        "status": status,
        "conclusion": conclusion,
        "run_attempt": j["attempt"].as_i64().unwrap_or(1),
        "workflow_name": run_name.map(Value::from).unwrap_or(Value::Null),
        "labels": j["runs_on"].clone(),
        "url": format!("{base}/actions/jobs/{id}"),
        "html_url": format!("{origin}/{owner}/{repo}/actions/runs/{run_id}/jobs/{id}"),
        "started_at": iso(&j["started_at"]),
        "completed_at": j["finished_at"].as_str().map(|_| iso(&j["finished_at"])).unwrap_or(Value::Null),
        "steps": [],
    })
}

/// `ActionWorkflowPublic` JSON → the GitHub workflow object. The path is the
/// identity (there is no separate numeric id); GitHub's `state` maps from our
/// `disabled` flag.
fn gh_workflow(origin: &str, owner: &str, repo: &str, w: &Value) -> Value {
    let path = w["path"].as_str().unwrap_or_default();
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": path,
        "name": w["name"].as_str().unwrap_or_default(),
        "path": path,
        "state": if w["disabled"].as_bool().unwrap_or(false) { "disabled_manually" } else { "active" },
        "url": format!("{base}/actions/workflows/{path}"),
        "html_url": format!("{origin}/{owner}/{repo}/actions?query=workflow%3A{path}"),
        "badge_url": format!("{base}/actions/workflows/{path}/badge.svg"),
    })
}

fn runs_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "workflow_runs": {"type": "array", "items": {"type": "object"}},
        },
        "required": ["total_count", "workflow_runs"],
    })
}

fn run_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn jobs_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "jobs": {"type": "array", "items": {"type": "object"}},
        },
        "required": ["total_count", "jobs"],
    })
}

fn job_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn workflows_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "workflows": {"type": "array", "items": {"type": "object"}},
        },
        "required": ["total_count", "workflows"],
    })
}

fn workflow_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn dispatch_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "ref": {"type": "string"},
            "inputs": {"type": "object"},
        },
        "required": ["ref"],
    })
}

/// GET /actions/runs — `gh run list`. Filters pass straight through to the
/// RPC (`status` includes `completed` → any terminal row).
async fn list_runs(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    Query(q): Query<RunsQuery>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    let mut input = repo_ref(&p.owner, &p.repo);
    input["actor"] = q.actor.as_deref().map_or(Value::Null, |v| json!(v));
    input["branch"] = q.branch.as_deref().map_or(Value::Null, |v| json!(v));
    input["event"] = q.event.as_deref().map_or(Value::Null, |v| json!(v));
    input["status"] = q.status.as_deref().map_or(Value::Null, |v| json!(v));
    input["per_page"] = q.per_page.map_or(Value::Null, |v| json!(v));
    input["page"] = q.page.map_or(Value::Null, |v| json!(v));
    call_map(
        &state,
        &headers,
        "repo.actions.listRuns",
        input,
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let runs: Vec<Value> = data["runs"]
                .as_array()
                .map(|rs| {
                    rs.iter()
                        .map(|r| gh_run(&origin, &owner, &repo, r))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": data["total_count"], "workflow_runs": runs})
        },
    )
    .await
}

/// GET /actions/runs/{run_id} — `gh run view`.
async fn get_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    call_map(
        &state,
        &headers,
        "repo.actions.getRun",
        p.ref_input(),
        StatusCode::OK,
        move |data| {
            gh_run(
                &crate::public_origin::resolve_public_origin(),
                &owner,
                &repo,
                &data["run"],
            )
        },
    )
    .await
}

/// GET /actions/runs/{run_id}/jobs — `gh run view`'s job table.
async fn list_run_jobs(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    call_map(
        &state,
        &headers,
        "repo.actions.getRun",
        p.ref_input(),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let run_name = data["run"]["workflow_name"].as_str().map(str::to_string);
            let jobs: Vec<Value> = data["jobs"]
                .as_array()
                .map(|js| {
                    js.iter()
                        .map(|j| gh_job(&origin, &owner, &repo, j, run_name.as_deref()))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": jobs.len(), "jobs": jobs})
        },
    )
    .await
}

/// GET /actions/jobs/{job_id} — `gh run view --job`.
async fn get_job(
    State(state): State<AppState>,
    Path(p): Path<JobPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    let input = json!({"owner": p.owner, "name": p.repo, "job_id": p.job_id});
    call_map(
        &state,
        &headers,
        "repo.actions.getJob",
        input,
        StatusCode::OK,
        move |data| {
            gh_job(
                &crate::public_origin::resolve_public_origin(),
                &owner,
                &repo,
                &data["job"],
                None,
            )
        },
    )
    .await
}

/// Shared POST-with-empty-body dispatch for run mutations.
async fn run_mutation(
    state: &AppState,
    headers: &HeaderMap,
    p: &RunPath,
    proc: &str,
    extra: Value,
    ok: StatusCode,
) -> Response {
    let mut input = p.ref_input();
    if let Some(obj) = extra.as_object() {
        for (k, v) in obj {
            input[k] = v.clone();
        }
    }
    call(state, headers, proc, input, ok).await
}

async fn rerun_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.rerunRun",
        json!({}),
        StatusCode::CREATED,
    )
    .await
}

async fn rerun_failed(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.rerunRun",
        json!({"failed_only": true}),
        StatusCode::CREATED,
    )
    .await
}

/// POST /actions/jobs/{job_id}/rerun — the per-job requeue resolves through
/// the job row to find its run first.
async fn rerun_job(
    State(state): State<AppState>,
    Path(p): Path<JobPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "job_id": p.job_id});
    let resp = dispatch(&mut ctx, "repo.actions.getJob", input).await;
    let job = match resp {
        RpcResponse::Ok { data, .. } => data["job"].clone(),
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let run_id = job["run_id"].as_str().unwrap_or_default().to_string();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["run_id"] = json!(run_id);
    input["job_id"] = json!(p.job_id);
    call(
        &state,
        &headers,
        "repo.actions.rerunRun",
        input,
        StatusCode::CREATED,
    )
    .await
}

async fn cancel_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.cancelRun",
        json!({}),
        StatusCode::ACCEPTED,
    )
    .await
}

async fn approve_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.approveRun",
        json!({}),
        StatusCode::CREATED,
    )
    .await
}

async fn delete_run(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    run_mutation(
        &state,
        &headers,
        &p,
        "repo.actions.deleteRun",
        json!({}),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// GET /actions/workflows — `gh workflow list`.
async fn list_workflows(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    call_map(
        &state,
        &headers,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let workflows: Vec<Value> = data["workflows"]
                .as_array()
                .map(|ws| {
                    ws.iter()
                        .map(|w| gh_workflow(&origin, &owner, &repo, w))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": workflows.len(), "workflows": workflows})
        },
    )
    .await
}

/// `workflow_id` resolves as full path, basename, or exact name — GitHub's
/// API accepts the workflow's numeric id or filename.
fn match_workflow<'a>(workflows: &'a [Value], id: &str) -> Option<&'a Value> {
    workflows.iter().find(|w| {
        let path = w["path"].as_str().unwrap_or_default();
        path == id || path.rsplit('/').next() == Some(id) || w["name"].as_str() == Some(id)
    })
}

/// GET /actions/workflows/{workflow_id} — `gh workflow view`.
async fn get_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo, wf_id) = (p.owner.clone(), p.repo.clone(), p.workflow_id.clone());
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
    )
    .await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let workflows = data["workflows"].as_array().cloned().unwrap_or_default();
    match match_workflow(&workflows, &wf_id) {
        Some(w) => Json(gh_workflow(
            &crate::public_origin::resolve_public_origin(),
            &owner,
            &repo,
            w,
        ))
        .into_response(),
        None => err_json(StatusCode::NOT_FOUND, "Workflow not found"),
    }
}

/// GET /actions/workflows/{workflow_id}/runs — `gh run list --workflow`.
async fn list_workflow_runs(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    Query(q): Query<RunsQuery>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo, wf_id) = (p.owner.clone(), p.repo.clone(), p.workflow_id.clone());
    // Resolve the id/name to the canonical path for the RPC filter.
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
    )
    .await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let workflows = data["workflows"].as_array().cloned().unwrap_or_default();
    let Some(w) = match_workflow(&workflows, &wf_id) else {
        return err_json(StatusCode::NOT_FOUND, "Workflow not found");
    };
    let path = w["path"].as_str().unwrap_or_default().to_string();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["workflow"] = json!(path);
    input["actor"] = q.actor.as_deref().map_or(Value::Null, |v| json!(v));
    input["branch"] = q.branch.as_deref().map_or(Value::Null, |v| json!(v));
    input["event"] = q.event.as_deref().map_or(Value::Null, |v| json!(v));
    input["status"] = q.status.as_deref().map_or(Value::Null, |v| json!(v));
    input["per_page"] = q.per_page.map_or(Value::Null, |v| json!(v));
    input["page"] = q.page.map_or(Value::Null, |v| json!(v));
    call_map(
        &state,
        &headers,
        "repo.actions.listRuns",
        input,
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let runs: Vec<Value> = data["runs"]
                .as_array()
                .map(|rs| {
                    rs.iter()
                        .map(|r| gh_run(&origin, &owner, &repo, r))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": data["total_count"], "workflow_runs": runs})
        },
    )
    .await
}

/// POST /actions/workflows/{workflow_id}/dispatches — `gh workflow run`.
async fn dispatch_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
    Json(body): Json<DispatchBody>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["workflow_id"] = json!(p.workflow_id);
    input["git_ref"] = json!(body.git_ref);
    if let Some(inputs) = body.inputs {
        input["inputs"] = inputs;
    }
    call(
        &state,
        &headers,
        "repo.actions.dispatchWorkflow",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

/// Shared enable/disable — `{owner, name, path, enabled}`.
async fn set_workflow_enabled(
    state: &AppState,
    headers: &HeaderMap,
    p: &WorkflowPath,
    enabled: bool,
) -> Response {
    let mut ctx = match ctx_for(state, headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "repo.actions.listWorkflows",
        repo_ref(&p.owner, &p.repo),
    )
    .await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let workflows = data["workflows"].as_array().cloned().unwrap_or_default();
    let Some(w) = match_workflow(&workflows, &p.workflow_id) else {
        return err_json(StatusCode::NOT_FOUND, "Workflow not found");
    };
    let mut input = repo_ref(&p.owner, &p.repo);
    input["path"] = json!(w["path"].as_str().unwrap_or_default());
    input["enabled"] = json!(enabled);
    call(
        state,
        headers,
        "repo.actions.setWorkflowEnabled",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn enable_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
) -> Response {
    set_workflow_enabled(&state, &headers, &p, true).await
}

async fn disable_workflow(
    State(state): State<AppState>,
    Path(p): Path<WorkflowPath>,
    headers: HeaderMap,
) -> Response {
    set_workflow_enabled(&state, &headers, &p, false).await
}

// ======================= secrets / variables / environments ===============
//
// GitHub's `gh secret`/`gh variable`/environment-admin REST shape over the
// repo/org RPC procs. Secret PUT accepts either plaintext `value` (our RPC
// shape) or `{encrypted_value, key_id}` sealed with libsodium
// `crypto_box_seal` against the matching `…/public-key` endpoint — the wire
// format `gh secret set`, `actions/github-script`, and GitHub's web UI emit.

fn secrets_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "secrets": {"type": "array", "items": secret_schema_value(false)},
        },
        "required": ["total_count", "secrets"],
    })
}

fn secret_schema(_gen: &mut SchemaGenerator) -> Value {
    secret_schema_value(false)
}

fn org_secret_schema(_gen: &mut SchemaGenerator) -> Value {
    secret_schema_value(true)
}

fn secret_schema_value(org: bool) -> Value {
    let mut props = json!({
        "name": {"type": "string"},
        "created_at": {"type": "string"},
        "updated_at": {"type": "string"},
    });
    if org {
        props["visibility"] = json!({"type": "string"});
        props["selected_repositories_url"] = json!({"type": "string"});
    }
    json!({"type": "object", "properties": props})
}

fn public_key_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {"key_id": {"type": "string"}, "key": {"type": "string"}},
        "required": ["key_id", "key"],
    })
}

fn variables_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "variables": {"type": "array", "items": variable_schema_value(false)},
        },
        "required": ["total_count", "variables"],
    })
}

fn variable_schema(_gen: &mut SchemaGenerator) -> Value {
    variable_schema_value(false)
}

fn org_variable_schema(_gen: &mut SchemaGenerator) -> Value {
    variable_schema_value(true)
}

fn variable_schema_value(org: bool) -> Value {
    let mut props = json!({
        "name": {"type": "string"},
        "value": {"type": "string"},
        "created_at": {"type": "string"},
        "updated_at": {"type": "string"},
    });
    if org {
        props["visibility"] = json!({"type": "string"});
        props["selected_repositories_url"] = json!({"type": "string"});
    }
    json!({"type": "object", "properties": props})
}

fn selected_repos_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "repositories": {"type": "array", "items": {"type": "object"}},
        },
    })
}

fn environments_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "environments": {"type": "array", "items": environment_schema_value()},
        },
    })
}

fn environment_schema(_gen: &mut SchemaGenerator) -> Value {
    environment_schema_value()
}

fn environment_schema_value() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {}, "name": {"type": "string"},
            "url": {"type": "string"}, "html_url": {"type": "string"},
            "created_at": {"type": "string"}, "updated_at": {"type": "string"},
            "protection_rules": {"type": "array", "items": {"type": "object"}},
            "deployment_branch_policy": {"type": "object"},
        },
    })
}

fn secret_put_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "encrypted_value": {"type": "string"},
            "key_id": {"type": "string"},
            "value": {"type": "string"},
            "visibility": {"type": "string"},
            "selected_repository_ids": {"type": "array", "items": {"type": "string"}},
        },
    })
}

fn variable_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string"}, "value": {"type": "string"},
            "visibility": {"type": "string"},
            "selected_repository_ids": {"type": "array", "items": {"type": "string"}},
        },
    })
}

fn env_upsert_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "wait_timer": {"type": "integer"},
            "prevent_self_review": {"type": "boolean"},
            "reviewers": {"type": "array", "items": {"type": "object"}},
            "deployment_branch_policy": {"type": "object"},
        },
    })
}

// ---- reshapers ----

fn gh_secret(s: &Value) -> Value {
    json!({
        "name": s["name"],
        "created_at": s["created_at"],
        "updated_at": s["updated_at"],
    })
}

fn gh_org_secret(origin: &str, org: &str, s: &Value) -> Value {
    let name = s["name"].as_str().unwrap_or_default();
    json!({
        "name": s["name"],
        "visibility": s["visibility"],
        "created_at": s["created_at"],
        "updated_at": s["updated_at"],
        "selected_repositories_url":
            format!("{origin}/api/v1/orgs/{org}/actions/secrets/{name}/selected-repositories"),
    })
}

fn gh_variable(v: &Value) -> Value {
    json!({
        "name": v["name"],
        "value": v["value"],
        "created_at": v["created_at"],
        "updated_at": v["updated_at"],
    })
}

fn gh_org_variable(origin: &str, org: &str, v: &Value) -> Value {
    let name = v["name"].as_str().unwrap_or_default();
    json!({
        "name": v["name"],
        "value": v["value"],
        "visibility": v["visibility"],
        "created_at": v["created_at"],
        "updated_at": v["updated_at"],
        "selected_repositories_url":
            format!("{origin}/api/v1/orgs/{org}/actions/variables/{name}/selected-repositories"),
    })
}

/// GitHub's environment object — our `protection` fields map onto the
/// `protection_rules` array + `deployment_branch_policy` flags.
fn gh_environment(origin: &str, owner: &str, repo: &str, e: &Value) -> Value {
    let name = e["name"].as_str().unwrap_or_default();
    let p = &e["protection"];
    let mut rules = Vec::new();
    let wait = p["wait_timer_minutes"].as_i64().unwrap_or(0);
    if wait > 0 {
        rules.push(json!({"type": "wait_timer", "wait_timer": wait}));
    }
    let reviewers = p["required_reviewers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let usernames = p["required_reviewer_usernames"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !reviewers.is_empty() {
        let rs: Vec<Value> = reviewers
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let login = usernames.get(i).cloned().unwrap_or(Value::Null);
                json!({"type": "User", "reviewer": {"id": id, "login": login}})
            })
            .collect();
        rules.push(json!({
            "type": "required_reviewers",
            "prevent_self_review": p["prevent_self_review"].as_bool().unwrap_or(false),
            "reviewers": rs,
        }));
    }
    let branches = p["deployment_branches"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !branches.is_empty() {
        rules.push(json!({"type": "branch_policy"}));
    }
    json!({
        "id": num_or_str(e["id"].as_str().unwrap_or_default()),
        "name": name,
        "url": format!("{origin}/api/v1/repos/{owner}/{repo}/environments/{name}"),
        "html_url": format!("{origin}/{owner}/{repo}/settings/environments/{name}"),
        "created_at": e["created_at"],
        "updated_at": e["updated_at"],
        "protection_rules": rules,
        "deployment_branch_policy": {
            "protected_branches": branches.is_empty(),
            "custom_branch_policies": !branches.is_empty(),
        },
    })
}

// ---- shared plumbing ----

/// Resolve the sealed-box plaintext for a secret PUT — `encrypted_value`
/// wins, falling back to plaintext `value` for first-party clients.
fn secret_value_from_body(body: &SecretPutBody, scope: &str) -> Result<String, Box<Response>> {
    if let Some(ev) = &body.encrypted_value {
        return crate::crypto::seal_open(crate::actions::secrets::KEY_ENVS, scope, ev)
            .map_err(|e| Box::new(err_json(StatusCode::BAD_REQUEST, &e)));
    }
    body.value.clone().ok_or_else(|| {
        Box::new(err_json(
            StatusCode::BAD_REQUEST,
            "value or encrypted_value is required",
        ))
    })
}

/// Dispatch a proc and render `{ok_status, Json(data)}` or the mapped error.
async fn call_dispatch(
    ctx: &mut RpcCtx,
    procedure: &str,
    input: Value,
    ok_status: StatusCode,
) -> Response {
    match dispatch(ctx, procedure, input).await {
        RpcResponse::Ok { data, .. } => (ok_status, Json(data)).into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// Dispatch a proc; on success run `f` over `data` and render.
async fn call_dispatch_map<F>(
    ctx: &mut RpcCtx,
    procedure: &str,
    input: Value,
    ok_status: StatusCode,
    f: F,
) -> Response
where
    F: FnOnce(Value) -> Value,
{
    match dispatch(ctx, procedure, input).await {
        RpcResponse::Ok { data, .. } => (ok_status, Json(f(data))).into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// Fetch one array member by `field == want` out of a list-proc response —
/// shared by the single-secret/variable GETs and the create-conflict checks.
async fn list_member_named(
    ctx: &mut RpcCtx,
    procedure: &str,
    input: Value,
    collection: &str,
    field: &str,
    want: &str,
) -> Result<Option<Value>, Box<Response>> {
    match dispatch(ctx, procedure, input).await {
        RpcResponse::Ok { data, .. } => Ok(data[collection]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .find(|m| m[field].as_str() == Some(want))
            .cloned()),
        RpcResponse::Err { error, .. } => Err(Box::new(
            (status_for_error(&error), Json(error)).into_response(),
        )),
    }
}

/// `resolve_repo_for_admin` under the edge ctx — the seal scope and the
/// proc's own gate share the same capability floor.
async fn admin_repo_id(ctx: &RpcCtx, owner: &str, repo: &str) -> Result<String, Box<Response>> {
    crate::repo::resolve_repo_for_admin(ctx, owner, repo)
        .await
        .map(|a| a.row.id)
        .map_err(|e| Box::new((status_for_error(&e), Json(e)).into_response()))
}

// ---- repo secrets ----

async fn list_repo_secrets(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(
        &mut ctx,
        "repo.actions.secrets.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |data| {
            let secrets: Vec<Value> = data["secrets"]
                .as_array()
                .map(|a| a.iter().map(gh_secret).collect())
                .unwrap_or_default();
            json!({"total_count": secrets.len(), "secrets": secrets})
        },
    )
    .await
}

async fn repo_public_key(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    call(
        &state,
        &headers,
        "repo.actions.secrets.publicKey",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
    )
    .await
}

async fn get_repo_secret(
    State(state): State<AppState>,
    Path(p): Path<SecretPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match list_member_named(
        &mut ctx,
        "repo.actions.secrets.list",
        repo_ref(&p.owner, &p.repo),
        "secrets",
        "name",
        &p.secret_name,
    )
    .await
    {
        Ok(Some(s)) => (StatusCode::OK, Json(gh_secret(&s))).into_response(),
        Ok(None) => err_json(StatusCode::NOT_FOUND, "Secret not found"),
        Err(r) => *r,
    }
}

async fn put_repo_secret(
    State(state): State<AppState>,
    Path(p): Path<SecretPath>,
    headers: HeaderMap,
    Json(body): Json<SecretPutBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let repo_id = match admin_repo_id(&ctx, &p.owner, &p.repo).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    // GitHub answers 201 on create, 204 on update.
    let existed = list_member_named(
        &mut ctx,
        "repo.actions.secrets.list",
        repo_ref(&p.owner, &p.repo),
        "secrets",
        "name",
        &p.secret_name,
    )
    .await
    .map(|o| o.is_some());
    let existed = match existed {
        Ok(e) => e,
        Err(r) => return *r,
    };
    let value = match secret_value_from_body(&body, &format!("repo:{repo_id}")) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let mut input = repo_ref(&p.owner, &p.repo);
    input["secret_name"] = json!(p.secret_name);
    input["value"] = json!(value);
    let status = if existed {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CREATED
    };
    call_dispatch(&mut ctx, "repo.actions.secrets.put", input, status).await
}

async fn delete_repo_secret(
    State(state): State<AppState>,
    Path(p): Path<SecretPath>,
    headers: HeaderMap,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["secret_name"] = json!(p.secret_name);
    call(
        &state,
        &headers,
        "repo.actions.secrets.delete",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- environment secrets ----

fn env_ref(p: &EnvPath) -> Value {
    json!({"owner": p.owner, "name": p.repo, "environment": p.environment_name})
}

async fn list_env_secrets(
    State(state): State<AppState>,
    Path(p): Path<EnvPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(
        &mut ctx,
        "repo.actions.environments.secretsList",
        env_ref(&p),
        StatusCode::OK,
        |data| {
            let secrets: Vec<Value> = data["secrets"]
                .as_array()
                .map(|a| a.iter().map(gh_secret).collect())
                .unwrap_or_default();
            json!({"total_count": secrets.len(), "secrets": secrets})
        },
    )
    .await
}

async fn env_public_key(
    State(state): State<AppState>,
    Path(p): Path<EnvPath>,
    headers: HeaderMap,
) -> Response {
    call(
        &state,
        &headers,
        "repo.actions.environments.secretsPublicKey",
        env_ref(&p),
        StatusCode::OK,
    )
    .await
}

async fn get_env_secret(
    State(state): State<AppState>,
    Path(p): Path<EnvSecretPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "environment": p.environment_name});
    match list_member_named(
        &mut ctx,
        "repo.actions.environments.secretsList",
        input,
        "secrets",
        "name",
        &p.secret_name,
    )
    .await
    {
        Ok(Some(s)) => (StatusCode::OK, Json(gh_secret(&s))).into_response(),
        Ok(None) => err_json(StatusCode::NOT_FOUND, "Secret not found"),
        Err(r) => *r,
    }
}

async fn put_env_secret(
    State(state): State<AppState>,
    Path(p): Path<EnvSecretPath>,
    headers: HeaderMap,
    Json(body): Json<SecretPutBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let repo_id = match admin_repo_id(&ctx, &p.owner, &p.repo).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "environment": p.environment_name});
    let existed = list_member_named(
        &mut ctx,
        "repo.actions.environments.secretsList",
        input.clone(),
        "secrets",
        "name",
        &p.secret_name,
    )
    .await
    .map(|o| o.is_some());
    let existed = match existed {
        Ok(e) => e,
        Err(r) => return *r,
    };
    let scope = format!("env:{repo_id}:{}", p.environment_name);
    let value = match secret_value_from_body(&body, &scope) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let mut input = input;
    input["secret_name"] = json!(p.secret_name);
    input["value"] = json!(value);
    let status = if existed {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CREATED
    };
    call_dispatch(
        &mut ctx,
        "repo.actions.environments.secretsPut",
        input,
        status,
    )
    .await
}

async fn delete_env_secret(
    State(state): State<AppState>,
    Path(p): Path<EnvSecretPath>,
    headers: HeaderMap,
) -> Response {
    let mut input = json!({
        "owner": p.owner, "name": p.repo,
        "environment": p.environment_name, "secret_name": p.secret_name,
    });
    let _ = &mut input;
    call(
        &state,
        &headers,
        "repo.actions.environments.secretsDelete",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- org secrets ----

fn org_ref(org: &str) -> Value {
    json!({"slug": org})
}

async fn list_org_secrets(
    State(state): State<AppState>,
    Path(p): Path<OrgPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let org = p.org.clone();
    call_dispatch_map(
        &mut ctx,
        "org.actions.secrets.list",
        org_ref(&p.org),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let secrets: Vec<Value> = data["secrets"]
                .as_array()
                .map(|a| a.iter().map(|s| gh_org_secret(&origin, &org, s)).collect())
                .unwrap_or_default();
            json!({"total_count": secrets.len(), "secrets": secrets})
        },
    )
    .await
}

async fn org_public_key(
    State(state): State<AppState>,
    Path(p): Path<OrgPath>,
    headers: HeaderMap,
) -> Response {
    call(
        &state,
        &headers,
        "org.actions.secrets.publicKey",
        org_ref(&p.org),
        StatusCode::OK,
    )
    .await
}

async fn get_org_secret(
    State(state): State<AppState>,
    Path(p): Path<OrgSecretPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match list_member_named(
        &mut ctx,
        "org.actions.secrets.list",
        org_ref(&p.org),
        "secrets",
        "name",
        &p.secret_name,
    )
    .await
    {
        Ok(Some(s)) => {
            let origin = crate::public_origin::resolve_public_origin();
            (StatusCode::OK, Json(gh_org_secret(&origin, &p.org, &s))).into_response()
        }
        Ok(None) => err_json(StatusCode::NOT_FOUND, "Secret not found"),
        Err(r) => *r,
    }
}

async fn put_org_secret(
    State(state): State<AppState>,
    Path(p): Path<OrgSecretPath>,
    headers: HeaderMap,
    Json(body): Json<SecretPutBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let org = match crate::org::load_org_by_slug(&ctx, &p.org).await {
        Ok(o) => o,
        Err(e) => return (status_for_error(&e), Json(e)).into_response(),
    };
    let existed = list_member_named(
        &mut ctx,
        "org.actions.secrets.list",
        org_ref(&p.org),
        "secrets",
        "name",
        &p.secret_name,
    )
    .await
    .map(|o| o.is_some());
    let existed = match existed {
        Ok(e) => e,
        Err(r) => return *r,
    };
    let value = match secret_value_from_body(&body, &format!("org:{}", org.id)) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let mut input = org_ref(&p.org);
    input["secret_name"] = json!(p.secret_name);
    input["value"] = json!(value);
    if let Some(v) = &body.visibility {
        input["visibility"] = json!(v);
    }
    if let Some(ids) = &body.selected_repository_ids {
        input["repository_ids"] = json!(ids);
    }
    let status = if existed {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CREATED
    };
    call_dispatch(&mut ctx, "org.actions.secrets.put", input, status).await
}

async fn delete_org_secret(
    State(state): State<AppState>,
    Path(p): Path<OrgSecretPath>,
    headers: HeaderMap,
) -> Response {
    let mut input = org_ref(&p.org);
    input["secret_name"] = json!(p.secret_name);
    call(
        &state,
        &headers,
        "org.actions.secrets.delete",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

/// Shared by the two org `…/selected-repositories` GETs — resolves each repo
/// id into GitHub's minimal `{id, name, full_name, private}` member shape.
async fn org_selected_repos(
    ctx: &mut RpcCtx,
    org: &str,
    procedure: &str,
    field: &str,
    name: &str,
) -> Response {
    let mut input = org_ref(org);
    input[field] = json!(name);
    match dispatch(ctx, procedure, input).await {
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
        RpcResponse::Ok { data, .. } => {
            let ids = data["repository_ids"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let mut repos = Vec::with_capacity(ids.len());
            for id in ids.iter().filter_map(|i| i.as_str()) {
                match ctx.db.find_repository_by_id(id).await {
                    Ok(Some(r)) => repos.push(json!({
                        "id": num_or_str(&r.id),
                        "name": r.name,
                        "full_name": format!("{org}/{}", r.name),
                        "private": r.visibility == "private",
                    })),
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, repo_id = %id, "selected-repo lookup failed")
                    }
                }
            }
            (
                StatusCode::OK,
                Json(json!({"total_count": repos.len(), "repositories": repos})),
            )
                .into_response()
        }
    }
}

async fn org_secret_selected_repos(
    State(state): State<AppState>,
    Path(p): Path<OrgSecretPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos(
        &mut ctx,
        &p.org,
        "org.actions.secrets.listRepos",
        "secret_name",
        &p.secret_name,
    )
    .await
}

async fn org_variable_selected_repos(
    State(state): State<AppState>,
    Path(p): Path<OrgVarPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos(
        &mut ctx,
        &p.org,
        "org.actions.variables.listRepos",
        "variable_name",
        &p.variable_name,
    )
    .await
}

// ---- repo variables ----

async fn list_repo_variables(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(
        &mut ctx,
        "repo.actions.variables.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |data| {
            let vars: Vec<Value> = data["variables"]
                .as_array()
                .map(|a| a.iter().map(gh_variable).collect())
                .unwrap_or_default();
            json!({"total_count": vars.len(), "variables": vars})
        },
    )
    .await
}

/// `POST …/variables` — GitHub answers 409 when the name is taken.
async fn create_repo_variable(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
    Json(body): Json<VariableBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let name = body.name.clone().unwrap_or_default();
    match list_member_named(
        &mut ctx,
        "repo.actions.variables.list",
        repo_ref(&p.owner, &p.repo),
        "variables",
        "name",
        &name,
    )
    .await
    {
        Err(r) => return *r,
        Ok(Some(_)) => return err_json(StatusCode::CONFLICT, "Variable already exists"),
        Ok(None) => {}
    }
    let value = match body.value.clone() {
        Some(v) => v,
        None => return err_json(StatusCode::BAD_REQUEST, "value is required"),
    };
    let mut input = repo_ref(&p.owner, &p.repo);
    input["variable_name"] = json!(name);
    input["value"] = json!(value);
    call_dispatch(
        &mut ctx,
        "repo.actions.variables.put",
        input,
        StatusCode::CREATED,
    )
    .await
}

async fn get_repo_variable(
    State(state): State<AppState>,
    Path(p): Path<VarPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match list_member_named(
        &mut ctx,
        "repo.actions.variables.list",
        repo_ref(&p.owner, &p.repo),
        "variables",
        "name",
        &p.variable_name,
    )
    .await
    {
        Ok(Some(v)) => (StatusCode::OK, Json(gh_variable(&v))).into_response(),
        Ok(None) => err_json(StatusCode::NOT_FOUND, "Variable not found"),
        Err(r) => *r,
    }
}

/// `PATCH …/variables/{name}` — 404 on a missing name (GitHub doesn't
/// upsert on PATCH).
async fn update_repo_variable(
    State(state): State<AppState>,
    Path(p): Path<VarPath>,
    headers: HeaderMap,
    Json(body): Json<VariableBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match list_member_named(
        &mut ctx,
        "repo.actions.variables.list",
        repo_ref(&p.owner, &p.repo),
        "variables",
        "name",
        &p.variable_name,
    )
    .await
    {
        Err(r) => return *r,
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "Variable not found"),
        Ok(Some(_)) => {}
    }
    let value = match body.value.clone() {
        Some(v) => v,
        None => return err_json(StatusCode::BAD_REQUEST, "value is required"),
    };
    let mut input = repo_ref(&p.owner, &p.repo);
    input["variable_name"] = json!(p.variable_name);
    input["value"] = json!(value);
    call_dispatch(
        &mut ctx,
        "repo.actions.variables.put",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn delete_repo_variable(
    State(state): State<AppState>,
    Path(p): Path<VarPath>,
    headers: HeaderMap,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["variable_name"] = json!(p.variable_name);
    call(
        &state,
        &headers,
        "repo.actions.variables.delete",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- environment variables ----

async fn list_env_variables(
    State(state): State<AppState>,
    Path(p): Path<EnvPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(
        &mut ctx,
        "repo.actions.environments.variablesList",
        env_ref(&p),
        StatusCode::OK,
        |data| {
            let vars: Vec<Value> = data["variables"]
                .as_array()
                .map(|a| a.iter().map(gh_variable).collect())
                .unwrap_or_default();
            json!({"total_count": vars.len(), "variables": vars})
        },
    )
    .await
}

async fn create_env_variable(
    State(state): State<AppState>,
    Path(p): Path<EnvPath>,
    headers: HeaderMap,
    Json(body): Json<VariableBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let name = body.name.clone().unwrap_or_default();
    match list_member_named(
        &mut ctx,
        "repo.actions.environments.variablesList",
        env_ref(&p),
        "variables",
        "name",
        &name,
    )
    .await
    {
        Err(r) => return *r,
        Ok(Some(_)) => return err_json(StatusCode::CONFLICT, "Variable already exists"),
        Ok(None) => {}
    }
    let value = match body.value.clone() {
        Some(v) => v,
        None => return err_json(StatusCode::BAD_REQUEST, "value is required"),
    };
    let mut input = env_ref(&p);
    input["variable_name"] = json!(name);
    input["value"] = json!(value);
    call_dispatch(
        &mut ctx,
        "repo.actions.environments.variablesPut",
        input,
        StatusCode::CREATED,
    )
    .await
}

async fn get_env_variable(
    State(state): State<AppState>,
    Path(p): Path<EnvVarPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "environment": p.environment_name});
    match list_member_named(
        &mut ctx,
        "repo.actions.environments.variablesList",
        input,
        "variables",
        "name",
        &p.variable_name,
    )
    .await
    {
        Ok(Some(v)) => (StatusCode::OK, Json(gh_variable(&v))).into_response(),
        Ok(None) => err_json(StatusCode::NOT_FOUND, "Variable not found"),
        Err(r) => *r,
    }
}

async fn update_env_variable(
    State(state): State<AppState>,
    Path(p): Path<EnvVarPath>,
    headers: HeaderMap,
    Json(body): Json<VariableBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let input = json!({"owner": p.owner, "name": p.repo, "environment": p.environment_name});
    match list_member_named(
        &mut ctx,
        "repo.actions.environments.variablesList",
        input.clone(),
        "variables",
        "name",
        &p.variable_name,
    )
    .await
    {
        Err(r) => return *r,
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "Variable not found"),
        Ok(Some(_)) => {}
    }
    let value = match body.value.clone() {
        Some(v) => v,
        None => return err_json(StatusCode::BAD_REQUEST, "value is required"),
    };
    let mut input = input;
    input["variable_name"] = json!(p.variable_name);
    input["value"] = json!(value);
    call_dispatch(
        &mut ctx,
        "repo.actions.environments.variablesPut",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn delete_env_variable(
    State(state): State<AppState>,
    Path(p): Path<EnvVarPath>,
    headers: HeaderMap,
) -> Response {
    let input = json!({
        "owner": p.owner, "name": p.repo,
        "environment": p.environment_name, "variable_name": p.variable_name,
    });
    call(
        &state,
        &headers,
        "repo.actions.environments.variablesDelete",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- org variables ----

async fn list_org_variables(
    State(state): State<AppState>,
    Path(p): Path<OrgPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let org = p.org.clone();
    call_dispatch_map(
        &mut ctx,
        "org.actions.variables.list",
        org_ref(&p.org),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let vars: Vec<Value> = data["variables"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|v| gh_org_variable(&origin, &org, v))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": vars.len(), "variables": vars})
        },
    )
    .await
}

async fn create_org_variable(
    State(state): State<AppState>,
    Path(p): Path<OrgPath>,
    headers: HeaderMap,
    Json(body): Json<VariableBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let name = body.name.clone().unwrap_or_default();
    match list_member_named(
        &mut ctx,
        "org.actions.variables.list",
        org_ref(&p.org),
        "variables",
        "name",
        &name,
    )
    .await
    {
        Err(r) => return *r,
        Ok(Some(_)) => return err_json(StatusCode::CONFLICT, "Variable already exists"),
        Ok(None) => {}
    }
    let value = match body.value.clone() {
        Some(v) => v,
        None => return err_json(StatusCode::BAD_REQUEST, "value is required"),
    };
    let mut input = org_ref(&p.org);
    input["variable_name"] = json!(name);
    input["value"] = json!(value);
    if let Some(v) = &body.visibility {
        input["visibility"] = json!(v);
    }
    if let Some(ids) = &body.selected_repository_ids {
        input["repository_ids"] = json!(ids);
    }
    call_dispatch(
        &mut ctx,
        "org.actions.variables.put",
        input,
        StatusCode::CREATED,
    )
    .await
}

async fn get_org_variable(
    State(state): State<AppState>,
    Path(p): Path<OrgVarPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match list_member_named(
        &mut ctx,
        "org.actions.variables.list",
        org_ref(&p.org),
        "variables",
        "name",
        &p.variable_name,
    )
    .await
    {
        Ok(Some(v)) => {
            let origin = crate::public_origin::resolve_public_origin();
            (StatusCode::OK, Json(gh_org_variable(&origin, &p.org, &v))).into_response()
        }
        Ok(None) => err_json(StatusCode::NOT_FOUND, "Variable not found"),
        Err(r) => *r,
    }
}

async fn update_org_variable(
    State(state): State<AppState>,
    Path(p): Path<OrgVarPath>,
    headers: HeaderMap,
    Json(body): Json<VariableBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match list_member_named(
        &mut ctx,
        "org.actions.variables.list",
        org_ref(&p.org),
        "variables",
        "name",
        &p.variable_name,
    )
    .await
    {
        Err(r) => return *r,
        Ok(None) => return err_json(StatusCode::NOT_FOUND, "Variable not found"),
        Ok(Some(_)) => {}
    }
    let value = match body.value.clone() {
        Some(v) => v,
        None => return err_json(StatusCode::BAD_REQUEST, "value is required"),
    };
    let mut input = org_ref(&p.org);
    input["variable_name"] = json!(p.variable_name);
    input["value"] = json!(value);
    if let Some(v) = &body.visibility {
        input["visibility"] = json!(v);
    }
    if let Some(ids) = &body.selected_repository_ids {
        input["repository_ids"] = json!(ids);
    }
    call_dispatch(
        &mut ctx,
        "org.actions.variables.put",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn delete_org_variable(
    State(state): State<AppState>,
    Path(p): Path<OrgVarPath>,
    headers: HeaderMap,
) -> Response {
    let mut input = org_ref(&p.org);
    input["variable_name"] = json!(p.variable_name);
    call(
        &state,
        &headers,
        "org.actions.variables.delete",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- environments ----

async fn list_environments(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    call_dispatch_map(
        &mut ctx,
        "repo.actions.environments.list",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            let envs: Vec<Value> = data["environments"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|e| gh_environment(&origin, &owner, &repo, e))
                        .collect()
                })
                .unwrap_or_default();
            json!({"total_count": envs.len(), "environments": envs})
        },
    )
    .await
}

async fn get_environment(
    State(state): State<AppState>,
    Path(p): Path<EnvPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    call_dispatch_map(
        &mut ctx,
        "repo.actions.environments.get",
        env_ref(&p),
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            gh_environment(&origin, &owner, &repo, &data["environment"])
        },
    )
    .await
}

async fn put_environment(
    State(state): State<AppState>,
    Path(p): Path<EnvPath>,
    headers: HeaderMap,
    Json(body): Json<EnvUpsertBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let mut protection = json!({});
    if let Some(w) = body.wait_timer {
        protection["wait_timer_minutes"] = json!(w.max(0));
    }
    if let Some(v) = body.prevent_self_review {
        protection["prevent_self_review"] = json!(v);
    }
    if let Some(reviewers) = &body.reviewers {
        let ids: Vec<String> = reviewers
            .iter()
            .filter_map(|r| {
                // GitHub reviewers look like `{type: "User", id: 42}` —
                // accept numeric ids, string ids, or `{reviewer: {login}}`.
                r["id"]
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| r["id"].as_i64().map(|n| n.to_string()))
                    .or_else(|| r["reviewer"]["login"].as_str().map(str::to_string))
            })
            .collect();
        protection["required_reviewers"] = json!(ids);
    }
    if let Some(dbp) = &body.deployment_branch_policy {
        if dbp["protected_branches"].as_bool() == Some(true) {
            protection["deployment_branches"] = json!([]);
        } else if let Some(branches) = dbp["branches"].as_array() {
            protection["deployment_branches"] = json!(branches);
        }
    }
    let mut input = env_ref(&p);
    input["protection"] = protection;
    let owner = p.owner.clone();
    let repo = p.repo.clone();
    call_dispatch_map(
        &mut ctx,
        "repo.actions.environments.upsert",
        input,
        StatusCode::OK,
        move |data| {
            let origin = crate::public_origin::resolve_public_origin();
            gh_environment(&origin, &owner, &repo, &data["environment"])
        },
    )
    .await
}

async fn delete_environment(
    State(state): State<AppState>,
    Path(p): Path<EnvPath>,
    headers: HeaderMap,
) -> Response {
    call(
        &state,
        &headers,
        "repo.actions.environments.delete",
        env_ref(&p),
        StatusCode::NO_CONTENT,
    )
    .await
}

fn selected_repos_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "selected_repository_ids": {"type": "array", "items": {"type": "string"}},
        },
        "required": ["selected_repository_ids"],
    })
}

#[derive(Deserialize)]
struct SelectedReposBody {
    selected_repository_ids: Option<Vec<String>>,
}

/// `PUT …/selected-repositories` — full replace of the allow set.
async fn org_selected_repos_replace(
    ctx: &mut RpcCtx,
    org: &str,
    set_proc: &str,
    field: &str,
    name: &str,
    ids: Vec<String>,
) -> Response {
    let mut input = org_ref(org);
    input[field] = json!(name);
    input["repository_ids"] = json!(ids);
    call_dispatch(ctx, set_proc, input, StatusCode::NO_CONTENT).await
}

/// Which org-scoped selected-repos set an add/remove call targets.
struct OrgSelTarget<'a> {
    org: &'a str,
    list_proc: &'a str,
    set_proc: &'a str,
    field: &'a str,
    name: &'a str,
}

/// `PUT/DELETE …/selected-repositories/{repository_id}` — add/remove one repo:
/// list the set, union or subtract, then write the replacement.
async fn org_selected_repos_one(
    ctx: &mut RpcCtx,
    t: &OrgSelTarget<'_>,
    repository_id: &str,
    add: bool,
) -> Response {
    let mut input = org_ref(t.org);
    input[t.field] = json!(t.name);
    let mut ids = match dispatch(ctx, t.list_proc, input).await {
        RpcResponse::Ok { data, .. } => data["repository_ids"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(|i| i.as_str().map(String::from))
            .collect::<Vec<_>>(),
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response()
        }
    };
    if add {
        if !ids.iter().any(|i| i == repository_id) {
            ids.push(repository_id.to_string());
        }
    } else {
        ids.retain(|i| i != repository_id);
    }
    org_selected_repos_replace(ctx, t.org, t.set_proc, t.field, t.name, ids).await
}

async fn org_secret_set_selected_repos(
    State(state): State<AppState>,
    Path(p): Path<OrgSecretPath>,
    headers: HeaderMap,
    Json(body): Json<SelectedReposBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos_replace(
        &mut ctx,
        &p.org,
        "org.actions.secrets.setRepos",
        "secret_name",
        &p.secret_name,
        body.selected_repository_ids.unwrap_or_default(),
    )
    .await
}

async fn org_secret_add_selected_repo(
    State(state): State<AppState>,
    Path(p): Path<OrgSecretRepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos_one(
        &mut ctx,
        &OrgSelTarget {
            org: &p.org,
            list_proc: "org.actions.secrets.listRepos",
            set_proc: "org.actions.secrets.setRepos",
            field: "secret_name",
            name: &p.secret_name,
        },
        &p.repository_id,
        true,
    )
    .await
}

async fn org_secret_remove_selected_repo(
    State(state): State<AppState>,
    Path(p): Path<OrgSecretRepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos_one(
        &mut ctx,
        &OrgSelTarget {
            org: &p.org,
            list_proc: "org.actions.secrets.listRepos",
            set_proc: "org.actions.secrets.setRepos",
            field: "secret_name",
            name: &p.secret_name,
        },
        &p.repository_id,
        false,
    )
    .await
}

async fn org_variable_set_selected_repos(
    State(state): State<AppState>,
    Path(p): Path<OrgVarPath>,
    headers: HeaderMap,
    Json(body): Json<SelectedReposBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos_replace(
        &mut ctx,
        &p.org,
        "org.actions.variables.setRepos",
        "variable_name",
        &p.variable_name,
        body.selected_repository_ids.unwrap_or_default(),
    )
    .await
}

async fn org_variable_add_selected_repo(
    State(state): State<AppState>,
    Path(p): Path<OrgVarRepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos_one(
        &mut ctx,
        &OrgSelTarget {
            org: &p.org,
            list_proc: "org.actions.variables.listRepos",
            set_proc: "org.actions.variables.setRepos",
            field: "variable_name",
            name: &p.variable_name,
        },
        &p.repository_id,
        true,
    )
    .await
}

async fn org_variable_remove_selected_repo(
    State(state): State<AppState>,
    Path(p): Path<OrgVarRepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    org_selected_repos_one(
        &mut ctx,
        &OrgSelTarget {
            org: &p.org,
            list_proc: "org.actions.variables.listRepos",
            set_proc: "org.actions.variables.setRepos",
            field: "variable_name",
            name: &p.variable_name,
        },
        &p.repository_id,
        false,
    )
    .await
}

// ---- deployments ----

/// `GET /deployments` filters — GitHub documents `sha`, `ref`, `task`,
/// `environment`.
#[derive(Deserialize, schemars::JsonSchema)]
struct DeploymentsQuery {
    sha: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    task: Option<String>,
    environment: Option<String>,
    per_page: Option<u32>,
    page: Option<u32>,
}

/// `POST /deployments` body — GitHub shape (`ref` renames to `git_ref`).
#[derive(Deserialize, schemars::JsonSchema)]
struct DeploymentCreateBody {
    #[serde(rename = "ref")]
    git_ref: String,
    task: Option<String>,
    auto_merge: Option<bool>,
    required_contexts: Option<Vec<String>>,
    payload: Option<Value>,
    environment: Option<String>,
    description: Option<String>,
    transient_environment: Option<bool>,
    production_environment: Option<bool>,
}

/// `POST /deployments/{id}/statuses` body — GitHub shape.
#[derive(Deserialize, schemars::JsonSchema)]
struct DeploymentStatusBody {
    state: String,
    description: Option<String>,
    environment: Option<String>,
    environment_url: Option<String>,
    target_url: Option<String>,
    log_url: Option<String>,
    auto_inactive: Option<bool>,
}

/// `POST /dispatches` body — `repository_dispatch`.
#[derive(Deserialize, schemars::JsonSchema)]
struct RepoDispatchBody {
    event_type: String,
    client_payload: Option<Value>,
}

/// `PUT /actions/permissions` body.
#[derive(Deserialize, schemars::JsonSchema)]
struct ActionsPermissionsPutBody {
    enabled: Option<bool>,
}

/// `PUT /actions/permissions/workflow` body.
#[derive(Deserialize, schemars::JsonSchema)]
struct WorkflowPermissionsBody {
    default_workflow_permissions: Option<String>,
    can_approve_pull_request_reviews: Option<bool>,
}

/// `POST …/pending_deployments` body — `{environment_ids, state, comment}`.
#[derive(Deserialize, schemars::JsonSchema)]
struct PendingDeploymentsBody {
    environment_ids: Vec<Value>,
    state: String,
    comment: Option<String>,
}

fn deployments_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn deployment_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn deployment_statuses_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": {"type": "object"}})
}

fn deployment_status_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "object"})
}

fn deployment_create_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["ref"],
        "properties": {
            "ref": {"type": "string"},
            "task": {"type": "string"},
            "auto_merge": {"type": "boolean"},
            "required_contexts": {"type": "array", "items": {"type": "string"}},
            "payload": {},
            "environment": {"type": "string"},
            "description": {"type": "string"},
            "transient_environment": {"type": "boolean"},
            "production_environment": {"type": "boolean"},
        },
    })
}

fn deployment_status_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["state"],
        "properties": {
            "state": {"type": "string"},
            "description": {"type": "string"},
            "environment": {"type": "string"},
            "environment_url": {"type": "string"},
            "target_url": {"type": "string"},
            "log_url": {"type": "string"},
            "auto_inactive": {"type": "boolean"},
        },
    })
}

fn repo_dispatch_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["event_type"],
        "properties": {
            "event_type": {"type": "string"},
            "client_payload": {"type": "object"},
        },
    })
}

fn actions_permissions_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "enabled": {"type": "boolean"},
            "allowed_actions": {"type": "string", "enum": ["all", "local_only", "selected"]},
        },
    })
}

fn actions_permissions_put_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "enabled": {"type": "boolean"},
            "allowed_actions": {"type": "string"},
        },
    })
}

fn workflow_permissions_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "default_workflow_permissions": {"type": "string", "enum": ["read", "write"]},
            "can_approve_pull_request_reviews": {"type": "boolean"},
        },
    })
}

fn pending_deployments_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({"type": "array", "items": environment_schema_value()})
}

fn pending_deployments_body_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "required": ["environment_ids", "state"],
        "properties": {
            "environment_ids": {"type": "array", "items": {}},
            "state": {"type": "string", "enum": ["approved", "rejected"]},
            "comment": {"type": "string"},
        },
    })
}

/// `ActionDeploymentPublic` JSON → the GitHub deployment object.
fn gh_deployment(origin: &str, owner: &str, repo: &str, d: &Value) -> Value {
    let id = d["id"].as_str().unwrap_or_default();
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": num_or_str(id),
        "node_id": "",
        "url": format!("{base}/deployments/{id}"),
        "statuses_url": format!("{base}/deployments/{id}/statuses"),
        "repository_url": base,
        "sha": d["sha"],
        "ref": d["ref"],
        "task": d["task"],
        "payload": d["payload"],
        "original_environment": d["environment"],
        "environment": d["environment"],
        "description": d["description"],
        "creator": d["creator_username"].as_str().map(|u| json!({
            "login": u, "type": "User", "site_admin": false,
        })),
        "transient_environment": d["transient_environment"],
        "production_environment": d["production_environment"],
        "created_at": d["created_at"],
        "updated_at": d["updated_at"],
    })
}

fn gh_deployment_status(origin: &str, owner: &str, repo: &str, st: &Value) -> Value {
    let id = st["id"].as_str().unwrap_or_default();
    let dep = st["deployment_id"].as_str().unwrap_or_default();
    let base = format!("{origin}/api/v1/repos/{owner}/{repo}");
    json!({
        "id": num_or_str(id),
        "node_id": "",
        "url": format!("{base}/deployments/{dep}/statuses/{id}"),
        "state": st["state"],
        "description": st["description"],
        "environment": st["environment"],
        "environment_url": st["environment_url"],
        "target_url": st["target_url"],
        "log_url": st["log_url"],
        "creator": st["creator_username"].as_str().map(|u| json!({
            "login": u, "type": "User", "site_admin": false,
        })),
        "created_at": st["created_at"],
        "updated_at": st["created_at"],
        "deployment_url": format!("{base}/deployments/{dep}"),
        "repository_url": base,
    })
}

async fn list_deployments(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    Query(q): Query<DeploymentsQuery>,
    headers: HeaderMap,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut input = repo_ref(&p.owner, &p.repo);
    if let Some(v) = &q.sha {
        input["sha"] = json!(v);
    }
    if let Some(v) = &q.git_ref {
        input["git_ref"] = json!(v);
    }
    if let Some(v) = &q.task {
        input["task"] = json!(v);
    }
    if let Some(v) = &q.environment {
        input["environment"] = json!(v);
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, "deployment.list", input).await {
        RpcResponse::Ok { data, .. } => {
            let items = data["deployments"].as_array().cloned().unwrap_or_default();
            let per_page = q.per_page.unwrap_or(30).clamp(1, 100) as usize;
            let start = (q.page.unwrap_or(1).max(1) - 1) as usize * per_page;
            let items = items
                .iter()
                .skip(start.min(items.len()))
                .take(per_page)
                .map(|d| gh_deployment(&origin, &p.owner, &p.repo, d))
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(json!(items))).into_response()
        }
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

async fn create_deployment(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
    Json(body): Json<DeploymentCreateBody>,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["ref"] = json!(body.git_ref);
    for (k, v) in [
        ("task", body.task.map(Value::from)),
        ("environment", body.environment.map(Value::from)),
        ("description", body.description.map(Value::from)),
        ("auto_merge", body.auto_merge.map(Value::from)),
        ("required_contexts", body.required_contexts.map(Value::from)),
        ("payload", body.payload),
        (
            "transient_environment",
            body.transient_environment.map(Value::from),
        ),
        (
            "production_environment",
            body.production_environment.map(Value::from),
        ),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k, v)))
    {
        input[k] = v;
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(
        &mut ctx,
        "deployment.create",
        input,
        StatusCode::CREATED,
        |d| gh_deployment(&origin, &p.owner, &p.repo, &d),
    )
    .await
}

async fn get_deployment(
    State(state): State<AppState>,
    Path(p): Path<DeploymentPath>,
    headers: HeaderMap,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["id"] = json!(p.deployment_id);
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(&mut ctx, "deployment.get", input, StatusCode::OK, |d| {
        gh_deployment(&origin, &p.owner, &p.repo, &d)
    })
    .await
}

async fn delete_deployment(
    State(state): State<AppState>,
    Path(p): Path<DeploymentPath>,
    headers: HeaderMap,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["id"] = json!(p.deployment_id);
    call(
        &state,
        &headers,
        "deployment.delete",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn list_deployment_statuses(
    State(state): State<AppState>,
    Path(p): Path<DeploymentPath>,
    headers: HeaderMap,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["deployment_id"] = json!(p.deployment_id);
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(&mut ctx, "deployment.status.list", input).await {
        RpcResponse::Ok { data, .. } => {
            let items = data["statuses"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|st| gh_deployment_status(&origin, &p.owner, &p.repo, st))
                .collect::<Vec<_>>();
            (StatusCode::OK, Json(json!(items))).into_response()
        }
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

async fn create_deployment_status(
    State(state): State<AppState>,
    Path(p): Path<DeploymentPath>,
    headers: HeaderMap,
    Json(body): Json<DeploymentStatusBody>,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut input = repo_ref(&p.owner, &p.repo);
    input["deployment_id"] = json!(p.deployment_id);
    input["state"] = json!(body.state);
    for (k, v) in [
        ("description", body.description.map(Value::from)),
        ("environment", body.environment.map(Value::from)),
        ("environment_url", body.environment_url.map(Value::from)),
        ("target_url", body.target_url.map(Value::from)),
        ("log_url", body.log_url.map(Value::from)),
        ("auto_inactive", body.auto_inactive.map(Value::from)),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k, v)))
    {
        input[k] = v;
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(
        &mut ctx,
        "deployment.status.create",
        input,
        StatusCode::CREATED,
        |st| gh_deployment_status(&origin, &p.owner, &p.repo, &st),
    )
    .await
}

// ---- repository_dispatch ----

async fn repository_dispatch(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
    Json(body): Json<RepoDispatchBody>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["event_type"] = json!(body.event_type);
    if let Some(cp) = body.client_payload {
        input["client_payload"] = cp;
    }
    call(
        &state,
        &headers,
        "repo.actions.dispatchRepository",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

// ---- actions permissions ----

async fn get_actions_permissions(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    call_dispatch_map(
        &mut ctx,
        "repo.actions.getEnabled",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        |d| {
            json!({
                "enabled": d["enabled"],
                // No selected-actions allowlist backing — the store models
                // enabled/disabled only, so the surface reads `all`.
                "allowed_actions": "all",
            })
        },
    )
    .await
}

async fn set_actions_permissions(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
    Json(body): Json<ActionsPermissionsPutBody>,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["enabled"] = json!(body.enabled.unwrap_or(true));
    call(
        &state,
        &headers,
        "repo.actions.setEnabled",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

async fn get_workflow_permissions(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let perms = match dispatch(
        &mut ctx,
        "repo.actions.getDefaultPermissions",
        repo_ref(&p.owner, &p.repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => data["default_permissions"]
            .as_str()
            .unwrap_or("read")
            .to_string(),
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response()
        }
    };
    let can_approve = match dispatch(
        &mut ctx,
        "repo.actions.getPullRequestTarget",
        repo_ref(&p.owner, &p.repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => data["enabled"].as_bool().unwrap_or(false),
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response()
        }
    };
    (
        StatusCode::OK,
        Json(json!({
            "default_workflow_permissions": perms,
            // Nearest backing toggle: pull_request_target opts Actions into
            // trusted-context work on PRs.
            "can_approve_pull_request_reviews": can_approve,
        })),
    )
        .into_response()
}

async fn set_workflow_permissions(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
    Json(body): Json<WorkflowPermissionsBody>,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    if let Some(dwp) = &body.default_workflow_permissions {
        let mut input = repo_ref(&p.owner, &p.repo);
        input["default_permissions"] = json!(dwp);
        match dispatch(&mut ctx, "repo.actions.setDefaultPermissions", input).await {
            RpcResponse::Ok { .. } => {}
            RpcResponse::Err { error, .. } => {
                return (status_for_error(&error), Json(error)).into_response()
            }
        }
    }
    if let Some(can) = body.can_approve_pull_request_reviews {
        let mut input = repo_ref(&p.owner, &p.repo);
        input["enabled"] = json!(can);
        match dispatch(&mut ctx, "repo.actions.setPullRequestTarget", input).await {
            RpcResponse::Ok { .. } => {}
            RpcResponse::Err { error, .. } => {
                return (status_for_error(&error), Json(error)).into_response()
            }
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

// ---- pending deployments (environment review gate) ----

/// Run's approval-held environment jobs → GitHub environment objects, with
/// `current_user_can_approve` from the job's reviewer gate.
async fn pending_envs_for_run(
    ctx: &mut RpcCtx,
    origin: &str,
    owner: &str,
    repo: &str,
    run_id: &str,
) -> Result<Vec<Value>, AppError> {
    let mut input = repo_ref(owner, repo);
    input["run_id"] = json!(run_id);
    let run = match dispatch(ctx, "repo.actions.getRun", input).await {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => return Err(error),
    };
    let mut held: HashMap<String, bool> = HashMap::new();
    for j in run["jobs"].as_array().cloned().unwrap_or_default() {
        if j["waiting_reason"].as_str() == Some("environment_review")
            && j["status"].as_str() == Some("waiting_approval")
        {
            if let Some(env) = j["environment"].as_str() {
                let can = held.entry(env.to_string()).or_insert(false);
                *can = *can || j["can_review"].as_bool().unwrap_or(false);
            }
        }
    }
    if held.is_empty() {
        return Ok(vec![]);
    }
    let envs = match dispatch(ctx, "repo.actions.environments.list", repo_ref(owner, repo)).await {
        RpcResponse::Ok { data, .. } => {
            data["environments"].as_array().cloned().unwrap_or_default()
        }
        RpcResponse::Err { error, .. } => return Err(error),
    };
    let mut out = Vec::new();
    for e in envs {
        let name = e["name"].as_str().unwrap_or_default();
        if let Some(can) = held.get(name) {
            let mut ge = gh_environment(origin, owner, repo, &e);
            ge["current_user_can_approve"] = json!(can);
            out.push(ge);
        }
    }
    Ok(out)
}

async fn list_pending_deployments(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match pending_envs_for_run(&mut ctx, &origin, &p.owner, &p.repo, &p.run_id).await {
        Ok(envs) => (StatusCode::OK, Json(json!(envs))).into_response(),
        Err(e) => (status_for_error(&e), Json(e)).into_response(),
    }
}

async fn review_pending_deployments(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
    Json(body): Json<PendingDeploymentsBody>,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    if !matches!(body.state.as_str(), "approved" | "rejected") {
        return err_json(
            StatusCode::BAD_REQUEST,
            "state must be 'approved' or 'rejected'",
        );
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    // environment_ids are GitHub ints; ours are string ids — resolve both to
    // environment names via the repo's environment list.
    let wanted: Vec<String> = body
        .environment_ids
        .iter()
        .filter_map(|v| {
            v.as_str()
                .map(String::from)
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
        .collect();
    let envs = match dispatch(
        &mut ctx,
        "repo.actions.environments.list",
        repo_ref(&p.owner, &p.repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => {
            data["environments"].as_array().cloned().unwrap_or_default()
        }
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response()
        }
    };
    let mut names = Vec::new();
    for e in &envs {
        let id = e["id"].as_str().unwrap_or_default();
        if wanted
            .iter()
            .any(|w| w == id || w == e["name"].as_str().unwrap_or_default())
        {
            names.push(e["name"].as_str().unwrap_or_default().to_string());
        }
    }
    // Held jobs in those environments → reviewDeployment per job.
    let mut input = repo_ref(&p.owner, &p.repo);
    input["run_id"] = json!(p.run_id);
    let run = match dispatch(&mut ctx, "repo.actions.getRun", input).await {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response()
        }
    };
    for j in run["jobs"].as_array().cloned().unwrap_or_default() {
        let env = j["environment"].as_str().unwrap_or_default();
        if j["waiting_reason"].as_str() == Some("environment_review")
            && j["status"].as_str() == Some("waiting_approval")
            && names.iter().any(|n| n == env)
        {
            let mut input = repo_ref(&p.owner, &p.repo);
            input["job_id"] = json!(j["id"].as_str().unwrap_or_default());
            input["state"] = json!(body.state);
            if let Some(c) = &body.comment {
                input["comment"] = json!(c);
            }
            match dispatch(&mut ctx, "repo.actions.reviewDeployment", input).await {
                RpcResponse::Ok { .. } => {}
                RpcResponse::Err { error, .. } => {
                    return (status_for_error(&error), Json(error)).into_response()
                }
            }
        }
    }
    // GitHub returns the environments the review covered.
    match pending_envs_for_run(&mut ctx, &origin, &p.owner, &p.repo, &p.run_id).await {
        Ok(_) => {}
        Err(e) => return (status_for_error(&e), Json(e)).into_response(),
    }
    let reviewed: Vec<Value> = envs
        .iter()
        .filter(|e| {
            names
                .iter()
                .any(|n| *n == e["name"].as_str().unwrap_or_default())
        })
        .map(|e| gh_environment(&origin, &p.owner, &p.repo, e))
        .collect();
    (StatusCode::OK, Json(json!(reviewed))).into_response()
}

// ---- caches + run attempts ----

fn caches_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "actions_caches": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "id": {},
                        "ref": {"type": "string"},
                        "key": {"type": "string"},
                        "version": {"type": "string"},
                        "last_accessed_at": {"type": "string"},
                        "created_at": {"type": "string"},
                        "size_in_bytes": {"type": "integer"},
                    },
                },
            },
        },
        "required": ["total_count", "actions_caches"],
    })
}

/// RPC cache row → GitHub `{id, ref, key, version, last_accessed_at,
/// created_at, size_in_bytes}`. `scope` is the branch ref the cache was
/// written under.
fn gh_cache(c: &Value) -> Value {
    json!({
        "id": c["id"],
        "ref": c["scope"],
        "key": c["key"],
        "version": c["version"],
        "last_accessed_at": c["last_accessed_at"],
        "created_at": c["created_at"],
        "size_in_bytes": c["size_bytes"],
    })
}

fn cache_matches(c: &Value, key: Option<&str>, git_ref: Option<&str>) -> bool {
    if let Some(k) = key {
        // GitHub matches on key prefix, not substring.
        if !c["key"].as_str().unwrap_or_default().starts_with(k) {
            return false;
        }
    }
    if let Some(r) = git_ref {
        if c["scope"].as_str().unwrap_or_default() != r {
            return false;
        }
    }
    true
}

/// GET /actions/caches — `gh cache list`.
async fn list_gh_caches(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    Query(q): Query<CachesQuery>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match dispatch(
        &mut ctx,
        "repo.actions.listCaches",
        repo_ref(&p.owner, &p.repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => {
            let mut items: Vec<Value> = data["caches"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .filter(|c| cache_matches(c, q.key.as_deref(), q.git_ref.as_deref()))
                .map(gh_cache)
                .collect();
            let sort_key = q.sort.as_deref().unwrap_or("last_accessed_at");
            let desc = q.direction.as_deref() != Some("asc");
            items.sort_by(|a, b| {
                let ord = match sort_key {
                    "size_in_bytes" => a["size_in_bytes"]
                        .as_i64()
                        .cmp(&b["size_in_bytes"].as_i64()),
                    // created_at | last_accessed_at | default
                    _ => a[sort_key]
                        .as_str()
                        .unwrap_or_default()
                        .cmp(b[sort_key].as_str().unwrap_or_default()),
                };
                if desc {
                    ord.reverse()
                } else {
                    ord
                }
            });
            let total_count = items.len();
            let per_page = q.per_page.unwrap_or(30).clamp(1, 100) as usize;
            let start = (q.page.unwrap_or(1).max(1) - 1) as usize * per_page;
            let page: Vec<Value> = items
                .into_iter()
                .skip(start.min(total_count))
                .take(per_page)
                .collect();
            (
                StatusCode::OK,
                Json(json!({"total_count": total_count, "actions_caches": page})),
            )
                .into_response()
        }
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// DELETE /actions/caches/{cache_id}.
async fn delete_gh_cache(
    State(state): State<AppState>,
    Path(p): Path<CachePath>,
    headers: HeaderMap,
) -> Response {
    let mut input = repo_ref(&p.owner, &p.repo);
    input["cache_id"] = json!(p.cache_id);
    call(
        &state,
        &headers,
        "repo.actions.deleteCache",
        input,
        StatusCode::NO_CONTENT,
    )
    .await
}

/// DELETE /actions/caches?key=&ref= — GitHub deletes every entry matching
/// the (prefix) key and optional ref.
async fn delete_gh_caches_by_key(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    Query(q): Query<CachesDeleteQuery>,
    headers: HeaderMap,
) -> Response {
    if q.key.is_none() && q.git_ref.is_none() {
        return err_json(StatusCode::BAD_REQUEST, "key or ref is required");
    }
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let caches = match dispatch(
        &mut ctx,
        "repo.actions.listCaches",
        repo_ref(&p.owner, &p.repo),
    )
    .await
    {
        RpcResponse::Ok { data, .. } => data["caches"].as_array().cloned().unwrap_or_default(),
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response()
        }
    };
    for c in caches
        .iter()
        .filter(|c| cache_matches(c, q.key.as_deref(), q.git_ref.as_deref()))
    {
        let mut input = repo_ref(&p.owner, &p.repo);
        input["cache_id"] = c["id"].clone();
        match dispatch(&mut ctx, "repo.actions.deleteCache", input).await {
            RpcResponse::Ok { .. } => {}
            RpcResponse::Err { error, .. } => {
                return (status_for_error(&error), Json(error)).into_response()
            }
        }
    }
    StatusCode::NO_CONTENT.into_response()
}

/// Fetch the run once and gate on `attempt_number` — reruns reuse job rows
/// (replacement semantics), so only the current attempt exists to serve.
async fn run_at_attempt(
    ctx: &mut RpcCtx,
    owner: &str,
    repo: &str,
    run_id: &str,
    attempt: i64,
) -> Result<Value, AppError> {
    let mut input = repo_ref(owner, repo);
    input["run_id"] = json!(run_id);
    let data = match dispatch(ctx, "repo.actions.getRun", input).await {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => return Err(error),
    };
    if data["run"]["attempt"].as_i64().unwrap_or(1) != attempt {
        return Err(AppError::new(
            "api.not_found",
            "attempt not found — earlier attempts are replaced on rerun",
        ));
    }
    Ok(data)
}

async fn get_run_attempt(
    State(state): State<AppState>,
    Path(p): Path<AttemptPath>,
    headers: HeaderMap,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match run_at_attempt(&mut ctx, &p.owner, &p.repo, &p.run_id, p.attempt_number).await {
        Ok(data) => (
            StatusCode::OK,
            Json(gh_run(&origin, &p.owner, &p.repo, &data["run"])),
        )
            .into_response(),
        Err(e) => (status_for_error(&e), Json(e)).into_response(),
    }
}

async fn list_attempt_jobs(
    State(state): State<AppState>,
    Path(p): Path<AttemptPath>,
    headers: HeaderMap,
) -> Response {
    let origin = crate::public_origin::resolve_public_origin();
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    match run_at_attempt(&mut ctx, &p.owner, &p.repo, &p.run_id, p.attempt_number).await {
        Ok(data) => {
            let run_name = data["run"]["workflow_name"].as_str().map(str::to_string);
            let jobs: Vec<Value> = data["jobs"]
                .as_array()
                .map(|js| {
                    js.iter()
                        .map(|j| gh_job(&origin, &p.owner, &p.repo, j, run_name.as_deref()))
                        .collect()
                })
                .unwrap_or_default();
            (
                StatusCode::OK,
                Json(json!({"total_count": jobs.len(), "jobs": jobs})),
            )
                .into_response()
        }
        Err(e) => (status_for_error(&e), Json(e)).into_response(),
    }
}

async fn download_attempt_logs(
    State(state): State<AppState>,
    Path(p): Path<AttemptPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    if let Err(e) = run_at_attempt(&mut ctx, &p.owner, &p.repo, &p.run_id, p.attempt_number).await {
        return (status_for_error(&e), Json(e)).into_response();
    }
    crate::routes::action_downloads::download_run_logs(
        State(state),
        axum::extract::Path((p.owner, p.repo, p.run_id)),
        headers,
    )
    .await
}

fn cache_usage_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "full_name": {"type": "string"},
            "active_caches_count": {"type": "integer"},
            "active_caches_size_in_bytes": {"type": "integer"},
        },
        "required": ["full_name", "active_caches_count", "active_caches_size_in_bytes"],
    })
}

/// DELETE /actions/runs/{run_id}/logs — drops stored logs, keeps the run.
async fn delete_run_logs_rest(
    State(state): State<AppState>,
    Path(p): Path<RunPath>,
    headers: HeaderMap,
) -> Response {
    call(
        &state,
        &headers,
        "repo.actions.deleteRunLogs",
        p.ref_input(),
        StatusCode::NO_CONTENT,
    )
    .await
}

/// GET /actions/cache/usage — `{full_name, active_caches_count,
/// active_caches_size_in_bytes}` off the listCaches rollup.
async fn get_cache_usage(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let (owner, repo) = (p.owner.clone(), p.repo.clone());
    call_map(
        &state,
        &headers,
        "repo.actions.listCaches",
        repo_ref(&p.owner, &p.repo),
        StatusCode::OK,
        move |data| {
            json!({
                "full_name": format!("{owner}/{repo}"),
                "active_caches_count": data["caches"].as_array().map(|c| c.len()).unwrap_or(0),
                "active_caches_size_in_bytes": data["total_size_bytes"].as_i64().unwrap_or(0),
            })
        },
    )
    .await
}

// --- self-hosted runners (repo scope) ---

#[derive(Deserialize)]
struct RunnerPath {
    owner: String,
    repo: String,
    runner_id: String,
}

impl RunnerPath {
    fn ref_input(&self) -> Value {
        json!({
            "owner": self.owner,
            "name": self.repo,
            "runner_id": self.runner_id,
        })
    }
}

fn runners_list_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "total_count": {"type": "integer"},
            "runners": {"type": "array", "items": runner_schema(_gen)},
        },
        "required": ["total_count", "runners"],
    })
}

fn runner_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string"},
            "name": {"type": "string"},
            "os": {"type": "string"},
            "status": {"type": "string", "enum": ["online", "offline"]},
            "busy": {"type": "boolean"},
            "ephemeral": {"type": "boolean"},
            "labels": {"type": "array", "items": {
                "type": "object",
                "properties": {
                    "id": {"type": "integer"},
                    "name": {"type": "string"},
                    "type": {"type": "string"},
                },
            }},
            "repository_id": {"type": "string"},
            "created_at": {"type": "string"},
        },
        "required": ["id", "name", "os", "status", "busy", "labels"],
    })
}

fn registration_token_schema(_gen: &mut SchemaGenerator) -> Value {
    json!({
        "type": "object",
        "properties": {
            "token": {"type": "string"},
            "expires_at": {"type": ["string", "null"]},
        },
        "required": ["token"],
    })
}

/// Online = heartbeat within the last 5 minutes (matches the runner liveness
/// window `touch_action_runner_online` maintains).
fn runner_online(last_online: Option<&str>) -> bool {
    last_online
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|t| {
            chrono::Utc::now() - t.with_timezone(&chrono::Utc) < chrono::Duration::minutes(5)
        })
}

/// Reshape `ActionRunnerPublic` to the GitHub runner object — `id` stays a
/// string (ours are uuids), labels gain GitHub's `{id, name, type}` entries.
fn gh_runner(origin: &str, owner: &str, repo: &str, r: &Value, busy: bool) -> Value {
    let id = r["id"].as_str().unwrap_or_default();
    let labels: Vec<Value> = r["labels"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, l)| {
            json!({
                "id": i as i64 + 1,
                "name": l.as_str().unwrap_or_default(),
                "type": "custom",
            })
        })
        .collect();
    let mut out = json!({
        "id": id,
        "name": r["name"],
        "os": r["os"].as_str().unwrap_or("unknown"),
        "status": if runner_online(r["last_online"].as_str()) { "online" } else { "offline" },
        "busy": busy,
        "ephemeral": r["ephemeral"].as_bool().unwrap_or(false),
        "labels": labels,
        "url": format!("{origin}/api/v1/repos/{owner}/{repo}/actions/runners/{id}"),
        "created_at": r["created_at"],
    });
    if let Some(rid) = r["repository_id"].as_str() {
        out["repository_id"] = json!(rid);
    }
    out
}

async fn gh_runner_full(state: &AppState, owner: &str, repo: &str, r: &Value) -> Value {
    let busy = state
        .db
        .action_runner_busy(r["id"].as_str().unwrap_or_default())
        .await
        .unwrap_or(false);
    gh_runner(
        &crate::public_origin::resolve_public_origin(),
        owner,
        repo,
        r,
        busy,
    )
}

/// GET /repos/{o}/{r}/actions/runners — `{total_count, runners}`.
async fn list_runners_rest(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(
        &mut ctx,
        "repo.actions.listRunners",
        repo_ref(&p.owner, &p.repo),
    )
    .await;
    let data = match resp {
        RpcResponse::Ok { data, .. } => data,
        RpcResponse::Err { error, .. } => {
            return (status_for_error(&error), Json(error)).into_response();
        }
    };
    let mut runners = Vec::new();
    for r in data["runners"].as_array().cloned().unwrap_or_default() {
        runners.push(gh_runner_full(&state, &p.owner, &p.repo, &r).await);
    }
    Json(json!({"total_count": runners.len(), "runners": runners})).into_response()
}

/// POST /repos/{o}/{r}/actions/runners/registration-token → 201 `{token}`.
async fn create_runner_registration_token(
    State(state): State<AppState>,
    Path(p): Path<RepoPath>,
    headers: HeaderMap,
) -> Response {
    call_map(
        &state,
        &headers,
        "repo.actions.createRegistrationToken",
        repo_ref(&p.owner, &p.repo),
        StatusCode::CREATED,
        move |data| json!({"token": data["token"], "expires_at": Value::Null}),
    )
    .await
}

/// GET /repos/{o}/{r}/actions/runners/{id}.
async fn get_runner_rest(
    State(state): State<AppState>,
    Path(p): Path<RunnerPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    let resp = dispatch(&mut ctx, "repo.actions.getRunner", p.ref_input()).await;
    match resp {
        RpcResponse::Ok { data, .. } => {
            let r = gh_runner_full(&state, &p.owner, &p.repo, &data["runner"]).await;
            Json(r).into_response()
        }
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}

/// DELETE /repos/{o}/{r}/actions/runners/{id} → 204.
async fn delete_runner_rest(
    State(state): State<AppState>,
    Path(p): Path<RunnerPath>,
    headers: HeaderMap,
) -> Response {
    let mut ctx = match ctx_for(&state, &headers).await {
        Ok(ctx) => ctx,
        Err(res) => return res,
    };
    match dispatch(&mut ctx, "repo.actions.deleteRunner", p.ref_input()).await {
        RpcResponse::Ok { .. } => StatusCode::NO_CONTENT.into_response(),
        RpcResponse::Err { error, .. } => (status_for_error(&error), Json(error)).into_response(),
    }
}
