//! `repo.actions.*` session RPC (ACT-03 / D-ACT-12 / D-ACT-13 / D-ACT-18 / ACT-06).

use std::collections::HashMap;

use uuid::Uuid;

use oxidean_core::{
    ActionDeploymentPublic, ActionDeploymentsListRequest, ActionDeploymentsListResponse,
    ActionDispatchRequest, ActionDispatchResponse, ActionEnabledRequest, ActionEnabledResponse,
    ActionEnvironmentProtection, ActionEnvironmentPublic, ActionJobApproveRequest,
    ActionJobApproveResponse, ActionJobLogRequest, ActionJobLogResponse, ActionJobPublic,
    ActionListRunnersResponse, ActionRegistrationTokenResponse, ActionRunGetRequest,
    ActionRunGetResponse, ActionRunMutationRequest, ActionRunMutationResponse, ActionRunPublic,
    ActionRunnerPublic, ActionRunsListRequest, ActionRunsListResponse, ActionSecretDeleteRequest,
    ActionSecretMetaPublic, ActionSecretPutRequest, ActionSecretsListRequest,
    ActionSecretsListResponse, ActionSetEnabledRequest, ActionWorkflowFileError,
    ActionWorkflowPublic, ActionWorkflowsListRequest, ActionWorkflowsListResponse, AppError,
    RepoActionsDefaultPermissionsRequest, RepoActionsDefaultPermissionsResponse,
    RepoActionsEnvironmentDeleteRequest, RepoActionsEnvironmentDeleteResponse,
    RepoActionsEnvironmentGetRequest, RepoActionsEnvironmentResponse,
    RepoActionsEnvironmentUpsertRequest, RepoActionsEnvironmentsListRequest,
    RepoActionsEnvironmentsListResponse, RepoActionsForkPrApprovalRequest,
    RepoActionsForkPrApprovalResponse, RepoActionsPullRequestTargetRequest,
    RepoActionsPullRequestTargetResponse, RepoActionsSetDefaultPermissionsRequest,
    RepoActionsSetForkPrApprovalRequest, RepoActionsSetPullRequestTargetRequest,
};
use oxidean_db::{ActionJobRow, ActionRunFilter, ActionRunRow, ActionRunnerRow};

use crate::actions::discover_workflows;
use crate::actions::dispatch::{bare_repo_path, enqueue_run};
use crate::actions::logs::read_job_log;
use crate::actions::secrets::{encrypt_secret, validate_secret_name};
use crate::actions::tokens::mint_registration_token;
use crate::auth::admin::require_admin as require_sys_admin;
use crate::auth::gate::require_verified;
use crate::repo::{meets, not_found, resolve_repo_for_admin, resolve_repo_for_read, Capability};
use crate::rpc::RpcCtx;

const RUNS_DEFAULT_PER_PAGE: u32 = 25;
const RUNS_MAX_PER_PAGE: u32 = 100;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "actions rpc db error");
        AppError::new("repo.internal", "repository operation failed")
    }
}

/// Coalesce the `approval_hold` flag into the public `waiting_approval`
/// status — terminal stored statuses always win (a cancelled held run is
/// `cancelled`, not `waiting_approval`).
fn public_status(status: &str, approval_hold: bool) -> String {
    if approval_hold && !matches!(status, "success" | "failure" | "cancelled" | "skipped") {
        "waiting_approval".into()
    } else {
        status.into()
    }
}

fn run_public(r: &ActionRunRow, actors: &HashMap<String, RunActor>) -> ActionRunPublic {
    let actor = r.triggered_by.as_ref().and_then(|id| actors.get(id));
    ActionRunPublic {
        id: r.id.clone(),
        repository_id: r.repository_id.clone(),
        workflow_path: r.workflow_path.clone(),
        workflow_name: r.workflow_name.clone(),
        event: r.event.clone(),
        head_sha: r.head_sha.clone(),
        head_ref: r.head_ref.clone(),
        status: public_status(&r.status, r.approval_hold),
        title: r.title.clone(),
        run_number: r.run_number,
        actor: actor.map(|a| a.username.clone()),
        actor_avatar_url: actor.and_then(|a| a.avatar_url.clone()),
        created_at: r.created_at.clone(),
        updated_at: r.updated_at.clone(),
        finished_at: r.finished_at.clone(),
    }
}

/// Resolved triggering actor for run serialization (username + avatar).
struct RunActor {
    username: String,
    avatar_url: Option<String>,
}

/// Batch-resolve `triggered_by` user ids to actors for run serialization.
async fn actors_for_runs(
    ctx: &RpcCtx,
    runs: &[ActionRunRow],
) -> Result<HashMap<String, RunActor>, AppError> {
    let ids: Vec<String> = runs.iter().filter_map(|r| r.triggered_by.clone()).collect();
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let mut unique = ids;
    unique.sort();
    unique.dedup();
    for u in ctx.db.find_users_by_ids(&unique).await.map_err(db_err)? {
        let avatar_url = u
            .avatar_path
            .as_ref()
            .map(|_| format!("/uploads/avatars/{}.webp", u.id));
        out.insert(
            u.id,
            RunActor {
                username: u.username,
                avatar_url,
            },
        );
    }
    Ok(out)
}

fn job_public(j: &ActionJobRow) -> ActionJobPublic {
    let runs_on: Vec<String> = serde_json::from_str(&j.runs_on_json).unwrap_or_default();
    ActionJobPublic {
        id: j.id.clone(),
        run_id: j.run_id.clone(),
        job_key: j.job_key.clone(),
        name: j.name.clone(),
        status: public_status(&j.status, j.approval_hold),
        runs_on,
        started_at: j.started_at.clone(),
        finished_at: j.finished_at.clone(),
        summary: j.summary_md.clone(),
    }
}

fn runner_public(r: &ActionRunnerRow) -> ActionRunnerPublic {
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

/// `repo.actions.listRuns` — Read+ (anonymous OK on public repos).
pub async fn list_runs(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunsListResponse, AppError> {
    let req: ActionRunsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.listRuns input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let page = req.page.unwrap_or(1).max(1);
    let per_page = req
        .per_page
        .unwrap_or(RUNS_DEFAULT_PER_PAGE)
        .clamp(1, RUNS_MAX_PER_PAGE);
    let offset = i64::from((page - 1) * per_page);

    // Optional GitHub-style filters (status/event/branch/workflow/actor/query).
    let nonempty = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let branch = nonempty(req.branch).map(|b| {
        b.strip_prefix("refs/heads/")
            .map(str::to_string)
            .unwrap_or(b)
    });
    let mut triggered_by = None;
    if let Some(actor) = nonempty(req.actor) {
        match ctx.db.find_user_by_username(&actor).await.map_err(db_err)? {
            Some(u) => triggered_by = Some(u.id),
            // Unknown actor — no runs can match; skip the list query entirely.
            None => {
                return Ok(ActionRunsListResponse {
                    runs: vec![],
                    total_count: 0,
                    page,
                    per_page,
                });
            }
        }
    }
    let filter = ActionRunFilter {
        status: nonempty(req.status),
        event: nonempty(req.event),
        branch,
        workflow_path: nonempty(req.workflow),
        triggered_by,
        title_like: nonempty(req.query).map(|q| format!("%{q}%")),
    };
    let (rows, total_count) = tokio::try_join!(
        async {
            ctx.db
                .list_action_runs_for_repo(&accessible.row.id, &filter, i64::from(per_page), offset)
                .await
        },
        async {
            ctx.db
                .count_action_runs_for_repo(&accessible.row.id, &filter)
                .await
        },
    )
    .map_err(db_err)?;
    let actors = actors_for_runs(ctx, &rows).await?;
    Ok(ActionRunsListResponse {
        runs: rows.iter().map(|r| run_public(r, &actors)).collect(),
        total_count,
        page,
        per_page,
    })
}

/// `repo.actions.getRun` — Read+ (anonymous OK on public repos).
pub async fn get_run(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunGetResponse, AppError> {
    let req: ActionRunGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.getRun input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let run = ctx
        .db
        .find_action_run_by_id(&req.run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if run.repository_id != accessible.row.id {
        return Err(not_found());
    }
    let jobs = ctx
        .db
        .list_action_jobs_for_run(&run.id)
        .await
        .map_err(db_err)?;
    let actors = actors_for_runs(ctx, std::slice::from_ref(&run)).await?;
    Ok(ActionRunGetResponse {
        run: run_public(&run, &actors),
        jobs: jobs.iter().map(job_public).collect(),
    })
}

/// `repo.actions.getJobLog` — Read+ (anonymous OK on public repos).
pub async fn get_job_log(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionJobLogResponse, AppError> {
    let req: ActionJobLogRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.getJobLog input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let run = ctx
        .db
        .find_action_run_by_id(&req.run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if run.repository_id != accessible.row.id {
        return Err(not_found());
    }
    let job = ctx
        .db
        .find_action_job_by_id(&req.job_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if job.run_id != run.id {
        return Err(not_found());
    }
    let (bytes, size) = read_job_log(
        &ctx.actions_log_dir,
        &req.run_id,
        &req.job_id,
        req.offset.unwrap_or(0),
    )
    .await
    .unwrap_or_default();
    Ok(ActionJobLogResponse {
        content: String::from_utf8_lossy(&bytes).into_owned(),
        next_offset: size,
        size,
    })
}

/// `repo.actions.secrets.list` — Admin; names only (D-ACT-17).
pub async fn list_secrets(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionSecretsListResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionSecretsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.secrets.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_action_secret_names(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(ActionSecretsListResponse {
        secrets: rows
            .into_iter()
            .map(|r| ActionSecretMetaPublic {
                name: r.name,
                updated_at: r.updated_at,
            })
            .collect(),
    })
}

/// `repo.actions.secrets.put` — Admin; value never returned on read (D-ACT-17).
pub async fn put_secret(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionSecretPutRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.secrets.put input: {e}"),
        )
    })?;
    validate_secret_name(&req.secret_name)
        .map_err(|msg| AppError::new("repo.actions.invalid_secret_name", msg))?;
    if req.value.is_empty() {
        return Err(AppError::new(
            "repo.actions.invalid_secret_value",
            "secret value is required",
        ));
    }
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let ciphertext = encrypt_secret(&req.value)
        .map_err(|e| AppError::new("repo.actions.secret_encrypt_failed", e))?;
    let _ = ctx
        .db
        .delete_action_secret_by_name(&accessible.row.id, &req.secret_name)
        .await
        .map_err(db_err)?;
    ctx.db
        .insert_action_secret(
            &Uuid::new_v4().to_string(),
            &accessible.row.id,
            &req.secret_name,
            &ciphertext,
        )
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true, "name": req.secret_name }))
}

/// `repo.actions.secrets.delete` — Admin.
pub async fn delete_secret(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionSecretDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.secrets.delete input: {e}"),
        )
    })?;
    validate_secret_name(&req.secret_name)
        .map_err(|msg| AppError::new("repo.actions.invalid_secret_name", msg))?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let ok = ctx
        .db
        .delete_action_secret_by_name(&accessible.row.id, &req.secret_name)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": ok }))
}

/// `repo.actions.getEnabled` — Read+ (anonymous OK on public repos).
pub async fn get_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionEnabledResponse, AppError> {
    let req: ActionEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.getEnabled input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let enabled = ctx
        .db
        .get_repo_actions_enabled(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(ActionEnabledResponse { enabled })
}

/// `repo.actions.setEnabled` — Admin (D-ACT-06).
pub async fn set_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionEnabledResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionSetEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.setEnabled input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .set_repo_actions_enabled(&accessible.row.id, req.enabled)
        .await
        .map_err(db_err)?;
    Ok(ActionEnabledResponse {
        enabled: req.enabled,
    })
}

/// `repo.actions.getPullRequestTarget` — Read+; repository opt-in for the
/// `pull_request_target` trigger.
pub async fn get_pull_request_target(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsPullRequestTargetResponse, AppError> {
    let req: RepoActionsPullRequestTargetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.getPullRequestTarget input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let enabled = ctx
        .db
        .get_repo_actions_pull_request_target(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsPullRequestTargetResponse { enabled })
}

/// `repo.actions.setPullRequestTarget` — Admin; enables trusted-context PR
/// workflows (`pull_request_target`). Off by default — opting in runs
/// workflow code from the *base* ref with full secrets on PR events, which is
/// why it requires Admin rather than Write.
pub async fn set_pull_request_target(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsPullRequestTargetResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsSetPullRequestTargetRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.setPullRequestTarget input: {e}"),
            )
        })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .set_repo_actions_pull_request_target(&accessible.row.id, req.enabled)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsPullRequestTargetResponse {
        enabled: req.enabled,
    })
}

/// `repo.actions.getForkPrApproval` — Read+; the fork PR run approval policy.
pub async fn get_fork_pr_approval(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsForkPrApprovalResponse, AppError> {
    let req: RepoActionsForkPrApprovalRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.getForkPrApproval input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let policy = ctx
        .db
        .get_repo_actions_fork_pr_approval(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsForkPrApprovalResponse { policy })
}

/// `repo.actions.setForkPrApproval` — Admin; `never` | `first-time` |
/// `always`. Controls whether `pull_request` runs from fork heads are held
/// in `waiting_approval` until a maintainer approves them.
pub async fn set_fork_pr_approval(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsForkPrApprovalResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsSetForkPrApprovalRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.setForkPrApproval input: {e}"),
        )
    })?;
    if !matches!(req.policy.as_str(), "never" | "first-time" | "always") {
        return Err(AppError::new(
            "rpc.bad_input",
            "policy must be one of: never, first-time, always",
        ));
    }
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .set_repo_actions_fork_pr_approval(&accessible.row.id, &req.policy)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsForkPrApprovalResponse { policy: req.policy })
}

/// `repo.actions.getDefaultPermissions` — Read+; the permission floor applied
/// when a workflow declares no `permissions:` block (`read` | `write`).
pub async fn get_default_permissions(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsDefaultPermissionsResponse, AppError> {
    let req: RepoActionsDefaultPermissionsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.getDefaultPermissions input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let default_permissions = ctx
        .db
        .get_repo_actions_default_permissions(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsDefaultPermissionsResponse {
        default_permissions,
    })
}

/// `repo.actions.setDefaultPermissions` — Admin; GitHub's "Workflow
/// permissions" setting (`read` restricted | `write` permissive).
pub async fn set_default_permissions(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsDefaultPermissionsResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsSetDefaultPermissionsRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.setDefaultPermissions input: {e}"),
            )
        })?;
    if !matches!(req.default_permissions.as_str(), "read" | "write") {
        return Err(AppError::new(
            "rpc.bad_input",
            "default_permissions must be one of: read, write",
        ));
    }
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    ctx.db
        .set_repo_actions_default_permissions(&accessible.row.id, &req.default_permissions)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsDefaultPermissionsResponse {
        default_permissions: req.default_permissions,
    })
}

/// Stored protection JSON → the typed public shape.
fn environment_public(r: &oxidean_db::ActionEnvironmentRow) -> ActionEnvironmentPublic {
    let raw: serde_json::Value =
        serde_json::from_str(&r.protection_json).unwrap_or_else(|_| serde_json::json!({}));
    let protection = ActionEnvironmentProtection {
        wait_timer_minutes: raw
            .get("wait_timer_minutes")
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            .max(0),
        required_reviewers: raw
            .get("required_reviewers")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        deployment_branches: raw
            .get("deployment_branches")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    };
    ActionEnvironmentPublic {
        name: r.name.clone(),
        protection,
        created_at: r.created_at.clone(),
        updated_at: r.updated_at.clone(),
    }
}

/// `repo.actions.environments.list` — Read+.
pub async fn list_environments(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsEnvironmentsListResponse, AppError> {
    let req: RepoActionsEnvironmentsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.environments.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let rows = ctx
        .db
        .list_action_environments(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsEnvironmentsListResponse {
        environments: rows.iter().map(environment_public).collect(),
    })
}

/// `repo.actions.environments.get` — Read+.
pub async fn get_environment(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsEnvironmentResponse, AppError> {
    let req: RepoActionsEnvironmentGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.environments.get input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let row = ctx
        .db
        .find_action_environment(&accessible.row.id, &req.environment)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(RepoActionsEnvironmentResponse {
        environment: environment_public(&row),
    })
}

/// `repo.actions.environments.upsert` — Admin; create or replace an
/// environment's protection rules. The env row is created on save when it
/// doesn't exist yet (workflows also auto-create rows on first reference).
pub async fn upsert_environment(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsEnvironmentResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentUpsertRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.environments.upsert input: {e}"),
        )
    })?;
    if req.environment.trim().is_empty() {
        return Err(AppError::new(
            "rpc.bad_input",
            "environment name is required",
        ));
    }
    if req.protection.wait_timer_minutes < 0 {
        return Err(AppError::new(
            "rpc.bad_input",
            "wait_timer_minutes must be non-negative",
        ));
    }
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    // Reviewers are user ids — drop unknown ids rather than storing a rule
    // that can never be satisfied.
    let mut reviewers = Vec::with_capacity(req.protection.required_reviewers.len());
    for uid in &req.protection.required_reviewers {
        match ctx.db.find_user_by_id(uid).await {
            Ok(Some(_)) => reviewers.push(uid.clone()),
            _ => {
                return Err(AppError::new(
                    "rpc.bad_input",
                    format!("unknown reviewer user id: {uid}"),
                ))
            }
        }
    }
    let protection_json = serde_json::json!({
        "wait_timer_minutes": req.protection.wait_timer_minutes,
        "required_reviewers": reviewers,
        "deployment_branches": req.protection.deployment_branches,
    })
    .to_string();
    ctx.db
        .upsert_action_environment(
            &Uuid::new_v4().to_string(),
            &accessible.row.id,
            req.environment.trim(),
            &protection_json,
        )
        .await
        .map_err(db_err)?;
    let row = ctx
        .db
        .find_action_environment(&accessible.row.id, req.environment.trim())
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(RepoActionsEnvironmentResponse {
        environment: environment_public(&row),
    })
}

/// `repo.actions.environments.delete` — Admin.
pub async fn delete_environment(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActionsEnvironmentDeleteResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.environments.delete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let deleted = ctx
        .db
        .delete_action_environment(&accessible.row.id, &req.environment)
        .await
        .map_err(db_err)?;
    Ok(RepoActionsEnvironmentDeleteResponse { deleted })
}

fn deployment_public(d: &oxidean_db::ActionDeploymentRow) -> ActionDeploymentPublic {
    ActionDeploymentPublic {
        id: d.id.clone(),
        environment: d.environment.clone(),
        run_id: d.run_id.clone(),
        job_id: d.job_id.clone(),
        sha: d.sha.clone(),
        git_ref: d.git_ref.clone(),
        url: d.url.clone(),
        job_status: d.job_status.clone(),
        created_at: d.created_at.clone(),
    }
}

/// `repo.actions.listDeployments` — Read+; environment deployment records
/// for a run.
pub async fn list_deployments(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionDeploymentsListResponse, AppError> {
    let req: ActionDeploymentsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.listDeployments input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let run = ctx
        .db
        .find_action_run_by_id(&req.run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if run.repository_id != accessible.row.id {
        return Err(not_found());
    }
    let rows = ctx
        .db
        .list_action_deployments_for_run(&req.run_id)
        .await
        .map_err(db_err)?;
    Ok(ActionDeploymentsListResponse {
        deployments: rows.iter().map(deployment_public).collect(),
    })
}

/// `repo.actions.approveJob` — Write+; release one approval-held job.
/// Deployment reviews are per-job (GitHub parity): when the job's
/// environment lists `required_reviewers`, the caller must be one of them —
/// or a repo Admin, who overrides reviewer membership the same way GitHub's
/// environment admins do.
pub async fn approve_job(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionJobApproveResponse, AppError> {
    let caller = require_verified(ctx).await?;
    let req: ActionJobApproveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.approveJob input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    let job = ctx
        .db
        .find_action_job_by_id(&req.job_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    let run = ctx
        .db
        .find_action_run_by_id(&job.run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if run.repository_id != accessible.row.id {
        return Err(not_found());
    }
    if !job.approval_hold {
        return Err(AppError::new(
            "repo.actions.job_not_held",
            "job is not waiting for approval",
        ));
    }
    // Required-reviewer membership: the hold is a deployment review only
    // when the env names reviewers — check the stored rule before releasing.
    if let Some(env_name) = job.environment_name.as_deref() {
        if let Some(env_row) = ctx
            .db
            .find_action_environment(&accessible.row.id, env_name)
            .await
            .map_err(db_err)?
        {
            let protection: serde_json::Value = serde_json::from_str(&env_row.protection_json)
                .unwrap_or_else(|_| serde_json::json!({}));
            let reviewers: Vec<String> = protection
                .get("required_reviewers")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if !reviewers.is_empty()
                && !reviewers.iter().any(|r| r == &caller.id)
                && !meets(accessible.capability, Capability::Admin)
            {
                return Err(AppError::new(
                    "repo.actions.not_a_reviewer",
                    "caller is not a required reviewer for this environment",
                ));
            }
        }
    }
    let released = ctx.db.approve_action_job(&job.id).await.map_err(db_err)?;
    if !released {
        return Err(AppError::new(
            "repo.actions.job_not_held",
            "job is not waiting for approval",
        ));
    }
    if let Err(e) = Box::pin(crate::actions::scheduler::recompute_ready_jobs(
        &ctx.db,
        &job.run_id,
    ))
    .await
    {
        tracing::warn!(error = %e, run_id = %job.run_id, "post-approval recompute failed");
    }
    if let Err(e) = ctx.db.recompute_action_run_status(&job.run_id).await {
        tracing::warn!(error = %e, run_id = %job.run_id, "post-approval rollup failed");
    }
    let job = ctx
        .db
        .find_action_job_by_id(&req.job_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(ActionJobApproveResponse {
        job: job_public(&job),
    })
}

/// `admin.actions.createRegistrationToken` — instance Admin; one-time token (D-ACT-08).
pub async fn admin_create_registration_token(
    ctx: &RpcCtx,
    _input: serde_json::Value,
) -> Result<ActionRegistrationTokenResponse, AppError> {
    require_sys_admin(ctx).await?;
    let token = mint_registration_token(&ctx.db)
        .await
        .map_err(|e| AppError::new("admin.actions.token_failed", e))?;
    Ok(ActionRegistrationTokenResponse {
        token,
        scope: "instance".into(),
    })
}

/// `admin.actions.listRunners` — instance Admin.
pub async fn admin_list_runners(
    ctx: &RpcCtx,
    _input: serde_json::Value,
) -> Result<ActionListRunnersResponse, AppError> {
    require_sys_admin(ctx).await?;
    let rows = ctx.db.list_action_runners().await.map_err(db_err)?;
    Ok(ActionListRunnersResponse {
        runners: rows.iter().map(runner_public).collect(),
    })
}

/// `repo.actions.listWorkflows` — Read+ (anonymous OK on public repos).
/// Discovers `.github/workflows/*.{yml,yaml}` at the given ref (default branch
/// when omitted) so the UI can offer the "Run workflow" dispatch picker.
pub async fn list_workflows(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionWorkflowsListResponse, AppError> {
    let req: ActionWorkflowsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.listWorkflows input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let bare = bare_repo_path(&ctx.repos_dir, &req.owner, &req.name);
    let treeish = resolve_treeish(
        ctx,
        &accessible.row.default_branch,
        req.git_ref.as_deref(),
        &bare,
    )
    .await?;
    let discovered = discover_workflows(ctx.git.as_ref(), &bare, &treeish)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "workflow discovery failed");
            AppError::new("repo.actions.discovery_failed", "failed to read workflows")
        })?;
    Ok(ActionWorkflowsListResponse {
        workflows: discovered
            .workflows
            .iter()
            .map(|w| ActionWorkflowPublic {
                path: w.path.clone(),
                name: w.document.name.clone(),
                supports_dispatch: w.document.triggers.workflow_dispatch,
            })
            .collect(),
        errors: discovered
            .errors
            .iter()
            .map(|e| ActionWorkflowFileError {
                path: e.path.clone(),
                message: e.message.clone(),
            })
            .collect(),
        git_ref: req
            .git_ref
            .unwrap_or_else(|| accessible.row.default_branch.clone()),
    })
}

/// `repo.actions.dispatchWorkflow` — Write+; enqueues a `workflow_dispatch` run.
pub async fn dispatch_workflow(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionDispatchResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ActionDispatchRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.dispatchWorkflow input: {e}"),
        )
    })?;
    if req.git_ref.trim().is_empty() || req.workflow_id.trim().is_empty() {
        return Err(AppError::new(
            "rpc.bad_input",
            "workflow_id and ref are required",
        ));
    }
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    if !ctx
        .db
        .get_repo_actions_enabled(&accessible.row.id)
        .await
        .map_err(db_err)?
    {
        return Err(AppError::new(
            "repo.actions.disabled",
            "actions are disabled for this repository",
        ));
    }
    let bare = bare_repo_path(&ctx.repos_dir, &req.owner, &req.name);
    let treeish = resolve_treeish(
        ctx,
        &accessible.row.default_branch,
        Some(req.git_ref.as_str()),
        &bare,
    )
    .await?;
    let discovered = discover_workflows(ctx.git.as_ref(), &bare, &treeish)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "workflow discovery failed");
            AppError::new("repo.actions.discovery_failed", "failed to read workflows")
        })?;
    let wanted = req.workflow_id.trim();
    let wf = discovered
        .workflows
        .iter()
        .find(|w| w.path == wanted || w.document.name == wanted);
    let wf = match wf {
        Some(w) => w,
        None => {
            if let Some(fe) = discovered
                .errors
                .iter()
                .find(|e| e.path == wanted || e.path.rsplit('/').next() == Some(wanted))
            {
                return Err(AppError::new(
                    "repo.actions.workflow_invalid",
                    format!("workflow is invalid: {}", fe.message),
                ));
            }
            return Err(AppError::new(
                "repo.actions.workflow_not_found",
                "workflow not found",
            ));
        }
    };
    if !wf.document.triggers.workflow_dispatch {
        return Err(AppError::new(
            "repo.actions.dispatch_unsupported",
            "workflow does not declare the workflow_dispatch trigger",
        ));
    }
    // `on.workflow_dispatch.inputs` — validate/coerce the caller's inputs
    // against the declared types; undeclared inputs pass through (GitHub
    // accepts extras and surfaces them in `github.event.inputs`).
    let inputs = resolve_dispatch_inputs(
        wf.document.triggers.dispatch_inputs.as_ref(),
        req.inputs.as_ref(),
    )
    .map_err(|msg| AppError::new("rpc.bad_input", msg))?;
    let head_ref = ref_display(&req.git_ref, &accessible.row.default_branch);
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let payload = crate::actions::payload::workflow_dispatch_payload(
        &head_ref,
        &inputs,
        &repository,
        &sender,
    );
    let (run_id, _jobs) = enqueue_run(
        &ctx.db,
        &accessible.row.id,
        &wf.path,
        &wf.document,
        "workflow_dispatch",
        &treeish,
        &head_ref,
        Some(&user.id),
        None,
        Some(&payload),
        false,
    )
    .await
    .map_err(db_err)?;
    Ok(ActionDispatchResponse {
        ok: true,
        run_id: Some(run_id),
    })
}

/// Validate/coerce `workflow_dispatch` inputs against the workflow's declared
/// `on.workflow_dispatch.inputs` map. GitHub semantics:
/// - `string` (default type)/`environment` → string values.
/// - `boolean` → JSON booleans (string "true"/"false" coerced).
/// - `number` → JSON numbers.
/// - `choice` → string in the declared `options` list.
/// - `required: true` with no `default` → must be supplied.
/// - Undeclared names pass through verbatim.
fn resolve_dispatch_inputs(
    declared: Option<&serde_json::Value>,
    supplied: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let mut out = serde_json::Map::new();
    if let Some(map) = supplied.and_then(|v| v.as_object()) {
        for (k, v) in map {
            out.insert(k.clone(), v.clone());
        }
    } else if supplied.is_some() {
        return Err("inputs must be an object".into());
    }
    let Some(declared) = declared.and_then(|v| v.as_object()) else {
        return Ok(serde_json::Value::Object(out));
    };
    for (name, spec) in declared {
        let spec = spec.as_object();
        let ty = spec
            .and_then(|s| s.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("string");
        let present = out.contains_key(name);
        if !present {
            let default = spec.and_then(|s| s.get("default")).cloned();
            match default {
                Some(d) => {
                    out.insert(name.clone(), d);
                }
                None => {
                    if spec
                        .and_then(|s| s.get("required"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        return Err(format!("input '{name}' is required"));
                    }
                    continue;
                }
            }
        }
        let v = out.get(name).cloned().unwrap_or(serde_json::Value::Null);
        let coerced = match ty {
            "boolean" => match &v {
                serde_json::Value::Bool(_) => v,
                serde_json::Value::String(s) if s == "true" || s == "false" => {
                    serde_json::Value::Bool(s == "true")
                }
                _ => return Err(format!("input '{name}' must be a boolean")),
            },
            "number" => match &v {
                serde_json::Value::Number(_) => v,
                serde_json::Value::String(s) => s
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| format!("input '{name}' must be a number"))?,
                _ => return Err(format!("input '{name}' must be a number")),
            },
            "choice" => {
                let opts: Vec<String> = spec
                    .and_then(|s| s.get("options"))
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|o| o.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                let sv = v.as_str().unwrap_or_default();
                if !opts.is_empty() && !opts.iter().any(|o| o == sv) {
                    return Err(format!(
                        "input '{name}' must be one of: {}",
                        opts.join(", ")
                    ));
                }
                v
            }
            // string / environment / unknown types — accept as-is (strings
            // are the norm; GitHub coerces scalars to strings, we keep the
            // caller's JSON type since expressions compare loosely).
            _ => v,
        };
        out.insert(name.clone(), coerced);
    }
    Ok(serde_json::Value::Object(out))
}

/// `repo.actions.approveRun` — Write+; releases a fork PR run held by the
/// `actions_fork_pr_approval` policy (clears `approval_hold`; the public
/// `waiting_approval` status is derived from that flag).
pub async fn approve_run(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunMutationResponse, AppError> {
    let (req, accessible, run) = run_mutation_target(ctx, input, "approveRun").await?;
    if !run.approval_hold {
        return Err(AppError::new(
            "repo.actions.run_not_held",
            "run is not waiting for approval",
        ));
    }
    ctx.db.approve_action_run(&run.id).await.map_err(db_err)?;
    if let Err(e) = Box::pin(crate::actions::scheduler::recompute_ready_jobs(
        &ctx.db, &run.id,
    ))
    .await
    {
        tracing::warn!(error = %e, run_id = %run.id, "post-approval recompute failed");
    }
    if let Err(e) = ctx.db.recompute_action_run_status(&run.id).await {
        tracing::warn!(error = %e, run_id = %run.id, "post-approval rollup failed");
    }
    run_mutation_response(ctx, &accessible.row.id, &req.run_id).await
}

/// `repo.actions.rerunRun` — Write+; requeues the run and all its jobs.
pub async fn rerun_run(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunMutationResponse, AppError> {
    let (req, accessible, _run) = run_mutation_target(ctx, input, "rerunRun").await?;
    // Optional targeting: `job_id` requeues a single job ("Re-run this job"),
    // `failed_only` requeues just failed/cancelled jobs ("Re-run failed jobs").
    if let Some(job_id) = req.job_id.as_deref() {
        let job = ctx
            .db
            .find_action_job_by_id(job_id)
            .await
            .map_err(db_err)?
            .ok_or_else(not_found)?;
        if job.run_id != req.run_id {
            return Err(not_found());
        }
    }
    ctx.db
        .requeue_action_run(
            &req.run_id,
            req.failed_only.unwrap_or(false),
            req.job_id.as_deref(),
        )
        .await
        .map_err(db_err)?;
    // Requeue cleared `timeout_at` — restart job deadlines from the
    // evaluated `timeout_seconds` captured in the durable payload, and let
    // the scheduler re-gate `waiting` needs-jobs whose upstreams stayed
    // terminal across the rerun.
    if let Ok(jobs) = ctx.db.list_action_jobs_for_run(&req.run_id).await {
        for job in &jobs {
            if !matches!(job.status.as_str(), "queued" | "waiting") {
                continue;
            }
            let secs = match job.payload_sha.as_deref() {
                Some(sha) => ctx
                    .db
                    .find_action_payload(sha)
                    .await
                    .ok()
                    .flatten()
                    .and_then(|p| serde_json::from_str::<serde_json::Value>(&p.payload_json).ok())
                    .and_then(|p| p.get("timeout_seconds").and_then(|v| v.as_f64())),
                None => None,
            };
            let deadline = secs.map(|s| {
                (chrono::Utc::now() + chrono::Duration::milliseconds((s * 1000.0) as i64))
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            });
            let _ = ctx
                .db
                .set_action_job_timeout_at(&job.id, deadline.as_deref())
                .await;
        }
        // Boxed: the scheduler future is large and this call sits inside the
        // monolithic rpc::dispatch future — keep its frame off the caller's.
        if let Err(e) = Box::pin(crate::actions::scheduler::recompute_ready_jobs(
            &ctx.db,
            &req.run_id,
        ))
        .await
        {
            tracing::warn!(error = %e, run_id = %req.run_id, "post-requeue ready-job recompute failed");
        }
        if let Err(e) = ctx.db.recompute_action_run_status(&req.run_id).await {
            tracing::warn!(error = %e, run_id = %req.run_id, "post-requeue rollup failed");
        }
    }
    // API-04: a requeued run is a new `workflow_run` `requested` delivery.
    crate::webhook::dispatch::notify_workflow_run(&ctx.db, &req.run_id, "requested", &ctx.env_name)
        .await;
    run_mutation_response(ctx, &accessible.row.id, &req.run_id).await
}

/// `repo.actions.cancelRun` — Write+; cancels run + unfinished jobs.
pub async fn cancel_run(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunMutationResponse, AppError> {
    let (req, accessible, run) = run_mutation_target(ctx, input, "cancelRun").await?;
    if run.finished_at.is_some() {
        return Err(AppError::new(
            "repo.actions.run_finished",
            "workflow run already finished",
        ));
    }
    let cancelled = ctx
        .db
        .cancel_action_run(&req.run_id)
        .await
        .map_err(db_err)?;
    if !cancelled {
        // Lost the race with a concurrent finish — surface the same error.
        return Err(AppError::new(
            "repo.actions.run_finished",
            "workflow run already finished",
        ));
    }
    // Revoke any OXIDEAN_TOKENs minted for this run's jobs (idempotent).
    if let Err(e) = ctx.db.revoke_action_job_tokens_for_run(&req.run_id).await {
        tracing::warn!(error = %e, run_id = %req.run_id, "failed to revoke job tokens");
    }
    // DEBT-06: cancelled runs complete — claim + emit through the watch matrix.
    if ctx
        .db
        .claim_run_completion_notice(&req.run_id)
        .await
        .unwrap_or(false)
    {
        if let Ok(Some(run)) = ctx.db.find_action_run_by_id(&req.run_id).await {
            crate::notify::fanout_workflow_completed(&ctx.db, &run).await;
        }
    }
    // API-04: cancellation completes the run with conclusion `cancelled`.
    crate::webhook::dispatch::notify_workflow_run(&ctx.db, &req.run_id, "completed", &ctx.env_name)
        .await;
    run_mutation_response(ctx, &accessible.row.id, &req.run_id).await
}

async fn run_mutation_target(
    ctx: &RpcCtx,
    input: serde_json::Value,
    proc: &str,
) -> Result<
    (
        ActionRunMutationRequest,
        crate::repo::AccessibleRepo,
        ActionRunRow,
    ),
    AppError,
> {
    let _ = require_verified(ctx).await?;
    let req: ActionRunMutationRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.{proc} input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    let run = ctx
        .db
        .find_action_run_by_id(&req.run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if run.repository_id != accessible.row.id {
        return Err(not_found());
    }
    Ok((req, accessible, run))
}

async fn run_mutation_response(
    ctx: &RpcCtx,
    repository_id: &str,
    run_id: &str,
) -> Result<ActionRunMutationResponse, AppError> {
    let run = ctx
        .db
        .find_action_run_by_id(run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    debug_assert_eq!(run.repository_id, repository_id);
    let actors = actors_for_runs(ctx, std::slice::from_ref(&run)).await?;
    Ok(ActionRunMutationResponse {
        run: run_public(&run, &actors),
    })
}

/// Resolve a user-supplied ref (branch/tag/SHA) or the default branch to a SHA.
pub(crate) async fn resolve_treeish(
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

/// Pretty ref for run rows: `main` for branch names, raw value otherwise.
fn ref_display(git_ref: &str, default_branch: &str) -> String {
    let r = git_ref.trim();
    let stripped = r.strip_prefix("refs/heads/").unwrap_or(r);
    if stripped.is_empty() {
        default_branch.to_string()
    } else {
        stripped.to_string()
    }
}
