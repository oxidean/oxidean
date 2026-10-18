//! `repo.actions.*` session RPC (ACT-03 / D-ACT-12 / D-ACT-13 / D-ACT-18 / ACT-06).

use std::collections::HashMap;

use uuid::Uuid;

use oxidean_core::{
    ActionAnnotationPublic, ActionArtifactDeleteRequest, ActionArtifactDeleteResponse,
    ActionArtifactPublic, ActionArtifactsListRequest, ActionArtifactsListResponse,
    ActionCacheDeleteRequest, ActionCacheDeleteResponse, ActionCachePublic,
    ActionCachesListRequest, ActionCachesListResponse, ActionDeploymentPublic,
    ActionDeploymentReviewPublic, ActionDeploymentReviewRequest, ActionDeploymentReviewResponse,
    ActionDeploymentsListRequest, ActionDeploymentsListResponse, ActionDispatchInputPublic,
    ActionDispatchRepositoryRequest, ActionDispatchRequest, ActionDispatchResponse,
    ActionEnabledRequest, ActionEnabledResponse, ActionEnvironmentProtection,
    ActionEnvironmentPublic, ActionJobAnnotationsRequest, ActionJobAnnotationsResponse,
    ActionJobApproveRequest, ActionJobApproveResponse, ActionJobLogRequest, ActionJobLogResponse,
    ActionJobPublic, ActionListRunnersResponse, ActionRegistrationTokenResponse,
    ActionRunGetRequest, ActionRunGetResponse, ActionRunMutationRequest, ActionRunMutationResponse,
    ActionRunPublic, ActionRunnerPublic, ActionRunsListRequest, ActionRunsListResponse,
    ActionSecretDeleteRequest, ActionSecretMetaPublic, ActionSecretPutRequest,
    ActionSecretsListRequest, ActionSecretsListResponse, ActionSetEnabledRequest,
    ActionSetWorkflowEnabledRequest, ActionVariablePublic, ActionVariablesDeleteRequest,
    ActionVariablesListRequest, ActionVariablesListResponse, ActionVariablesPutRequest,
    ActionWorkflowFileError, ActionWorkflowPublic, ActionWorkflowsListRequest,
    ActionWorkflowsListResponse, AppError, RepoActionsDefaultPermissionsRequest,
    RepoActionsDefaultPermissionsResponse, RepoActionsEnvironmentDeleteRequest,
    RepoActionsEnvironmentDeleteResponse, RepoActionsEnvironmentGetRequest,
    RepoActionsEnvironmentResponse, RepoActionsEnvironmentSecretDeleteRequest,
    RepoActionsEnvironmentSecretPutRequest, RepoActionsEnvironmentSecretsListRequest,
    RepoActionsEnvironmentUpsertRequest, RepoActionsEnvironmentVariableDeleteRequest,
    RepoActionsEnvironmentVariablePutRequest, RepoActionsEnvironmentVariablesListRequest,
    RepoActionsEnvironmentsListRequest, RepoActionsEnvironmentsListResponse,
    RepoActionsForkPrApprovalRequest, RepoActionsForkPrApprovalResponse,
    RepoActionsPullRequestTargetRequest, RepoActionsPullRequestTargetResponse,
    RepoActionsSetDefaultPermissionsRequest, RepoActionsSetForkPrApprovalRequest,
    RepoActionsSetPullRequestTargetRequest,
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

fn run_public(
    r: &ActionRunRow,
    actors: &HashMap<String, RunActor>,
    capability: Option<Capability>,
) -> ActionRunPublic {
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
        attempt: r.attempt,
        waiting_reason: r.approval_hold.then(|| "fork_approval".to_string()),
        can_approve: r.approval_hold && meets(capability, Capability::Write),
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

/// Caller-scoped data for job serialization — environment reviewer rules,
/// the caller's identity/capability (for `can_review`), the run's triggering
/// actor (for `prevent_self_review`), and recorded review history.
struct JobViewCtx<'a> {
    capability: Option<Capability>,
    caller_id: Option<&'a str>,
    triggered_by: Option<&'a str>,
    /// env name → required reviewer user ids (missing/empty = no review gate).
    env_reviewers: HashMap<String, Vec<String>>,
    env_prevent_self_review: HashMap<String, bool>,
    /// job id → recorded deployment reviews, oldest first.
    reviews: HashMap<String, Vec<ActionDeploymentReviewPublic>>,
}

impl JobViewCtx<'_> {
    /// Required reviewer user ids for a job's environment (empty when the job
    /// has no env or the env names no reviewers).
    fn reviewers_for(&self, j: &ActionJobRow) -> &[String] {
        j.environment_name
            .as_deref()
            .and_then(|e| self.env_reviewers.get(e))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// GitHub's `prevent_self_review`: the run's triggering actor may not
    /// review their own deployment when the environment sets the flag.
    fn self_review_blocked(&self, j: &ActionJobRow) -> bool {
        let blocked = j
            .environment_name
            .as_deref()
            .and_then(|e| self.env_prevent_self_review.get(e))
            .copied()
            .unwrap_or(false);
        blocked && self.caller_id.is_some() && self.caller_id == self.triggered_by
    }
}

fn waiting_reason(j: &ActionJobRow, view: &JobViewCtx<'_>) -> Option<String> {
    if j.approval_hold {
        return Some(
            if view.reviewers_for(j).is_empty() {
                "fork_approval"
            } else {
                "environment_review"
            }
            .to_string(),
        );
    }
    if j.status != "waiting" {
        return None;
    }
    if j.deployment_not_before.is_some() {
        return Some("environment_wait_timer".into());
    }
    Some("needs".into())
}

fn job_public(j: &ActionJobRow, view: &JobViewCtx<'_>) -> ActionJobPublic {
    let runs_on: Vec<String> = serde_json::from_str(&j.runs_on_json).unwrap_or_default();
    let reviewers = view.reviewers_for(j);
    let is_reviewer = view
        .caller_id
        .is_some_and(|id| reviewers.iter().any(|r| r == id));
    let can_review = j.approval_hold
        && !reviewers.is_empty()
        && (is_reviewer || meets(view.capability, Capability::Admin))
        && !view.self_review_blocked(j);
    ActionJobPublic {
        id: j.id.clone(),
        run_id: j.run_id.clone(),
        job_key: j.job_key.clone(),
        name: j.name.clone(),
        status: public_status(&j.status, j.approval_hold),
        runs_on,
        needs: serde_json::from_str(&j.needs_json).unwrap_or_default(),
        matrix: serde_json::from_str(&j.matrix_json).unwrap_or(serde_json::json!({})),
        environment: j.environment_name.clone(),
        attempt: j.attempt,
        waiting_reason: waiting_reason(j, view),
        can_review,
        reviews: view.reviews.get(&j.id).cloned().unwrap_or_default(),
        started_at: j.started_at.clone(),
        finished_at: j.finished_at.clone(),
        summary: j.summary_md.clone(),
    }
}

/// Parse an environment row's protection JSON into the reviewer list and
/// `prevent_self_review` flag the view context needs.
fn env_protection_parts(row: &oxidean_db::ActionEnvironmentRow) -> (Vec<String>, bool) {
    let raw: serde_json::Value =
        serde_json::from_str(&row.protection_json).unwrap_or_else(|_| serde_json::json!({}));
    let reviewers = raw
        .get("required_reviewers")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let prevent = raw
        .get("prevent_self_review")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    (reviewers, prevent)
}

/// Build the caller-scoped view for a run's jobs — one env lookup per unique
/// environment name plus the run's recorded deployment reviews.
async fn job_view_ctx<'a>(
    ctx: &'a RpcCtx,
    repo_id: &str,
    run: &'a ActionRunRow,
    jobs: &[ActionJobRow],
    capability: Option<Capability>,
) -> Result<JobViewCtx<'a>, AppError> {
    let mut view = JobViewCtx {
        capability,
        caller_id: ctx.session.as_ref().map(|s| s.user_id.as_str()),
        triggered_by: run.triggered_by.as_deref(),
        env_reviewers: HashMap::new(),
        env_prevent_self_review: HashMap::new(),
        reviews: HashMap::new(),
    };
    let mut envs: Vec<String> = jobs
        .iter()
        .filter_map(|j| j.environment_name.clone())
        .collect();
    envs.sort();
    envs.dedup();
    for env in envs {
        if let Some(row) = ctx
            .db
            .find_action_environment(repo_id, &env)
            .await
            .map_err(db_err)?
        {
            let (reviewers, prevent) = env_protection_parts(&row);
            view.env_reviewers.insert(env.clone(), reviewers);
            view.env_prevent_self_review.insert(env, prevent);
        }
    }
    for r in ctx
        .db
        .list_deployment_reviews_for_run(&run.id)
        .await
        .map_err(db_err)?
    {
        view.reviews
            .entry(r.job_id.clone())
            .or_default()
            .push(ActionDeploymentReviewPublic {
                reviewer: r.reviewer_username,
                state: r.state,
                comment: r.comment,
                created_at: r.created_at,
            });
    }
    Ok(view)
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
        runs: rows
            .iter()
            .map(|r| run_public(r, &actors, accessible.capability))
            .collect(),
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
    let view = job_view_ctx(ctx, &accessible.row.id, &run, &jobs, accessible.capability).await?;
    Ok(ActionRunGetResponse {
        run: run_public(&run, &actors, accessible.capability),
        jobs: jobs.iter().map(|j| job_public(j, &view)).collect(),
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
    let offset = req.offset.unwrap_or(0);
    let (bytes, size) = read_job_log(&ctx.actions_log_dir, &req.run_id, &req.job_id, offset)
        .await
        .unwrap_or_default();
    // Live-tail long-poll (Phase 8): nothing new and the job is still live →
    // hold the request until bytes land, the job goes terminal, or the wait
    // cap expires — the UI's log viewer gets sub-second updates without
    // fixed-cadence polling traffic.
    if bytes.is_empty() {
        let wait = req.wait_ms.unwrap_or(0).min(30_000);
        if wait > 0 && !job_status_terminal(&job.status) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(wait);
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let (b, s) = read_job_log(&ctx.actions_log_dir, &req.run_id, &req.job_id, offset)
                    .await
                    .unwrap_or_default();
                let live = ctx
                    .db
                    .find_action_job_by_id(&req.job_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some_and(|j| !job_status_terminal(&j.status));
                if !b.is_empty() || !live || std::time::Instant::now() >= deadline {
                    return Ok(ActionJobLogResponse {
                        content: String::from_utf8_lossy(&b).into_owned(),
                        next_offset: s,
                        size: s,
                        done: !live,
                    });
                }
            }
        }
    }
    Ok(ActionJobLogResponse {
        content: String::from_utf8_lossy(&bytes).into_owned(),
        next_offset: size,
        size,
        done: job_status_terminal(&job.status),
    })
}

fn job_status_terminal(status: &str) -> bool {
    matches!(status, "success" | "failure" | "cancelled" | "skipped")
}

/// `repo.actions.jobAnnotations` — Read+ (anonymous OK on public repos).
/// Annotations a run's jobs emitted via `::error|warning|notice::`.
pub async fn job_annotations(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionJobAnnotationsResponse, AppError> {
    let req: ActionJobAnnotationsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.jobAnnotations input: {e}"),
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
    let rows = match &req.job_id {
        Some(job_id) => {
            let job = ctx
                .db
                .find_action_job_by_id(job_id)
                .await
                .map_err(db_err)?
                .ok_or_else(not_found)?;
            if job.run_id != run.id {
                return Err(not_found());
            }
            ctx.db
                .list_action_annotations_for_job(job_id)
                .await
                .map_err(db_err)?
        }
        None => ctx
            .db
            .list_action_annotations_for_run(&req.run_id)
            .await
            .map_err(db_err)?,
    };
    Ok(ActionJobAnnotationsResponse {
        annotations: rows
            .into_iter()
            .map(|r| ActionAnnotationPublic {
                annotation_level: r.annotation_level,
                message: r.message,
                path: r.path,
                start_line: r.start_line,
                end_line: r.end_line,
                start_column: r.start_column,
                end_column: r.end_column,
                title: r.title,
                created_at: r.created_at,
            })
            .collect(),
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
    crate::actions::secrets::validate_secret_value(&req.value)
        .map_err(|msg| AppError::new("repo.actions.invalid_secret_value", msg))?;
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

/// `repo.actions.variables.list` — Admin; values returned (variables are
/// plaintext config, not secrets — GitHub GET returns them too).
pub async fn list_variables(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionVariablesListResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionVariablesListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.variables.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let rows = ctx
        .db
        .list_action_variables(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(ActionVariablesListResponse {
        variables: rows
            .into_iter()
            .map(|r| ActionVariablePublic {
                name: r.name,
                value: r.value,
                updated_at: r.updated_at,
            })
            .collect(),
    })
}

/// `repo.actions.variables.put` — Admin; creates or replaces a variable.
pub async fn put_variable(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionVariablesPutRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.variables.put input: {e}"),
        )
    })?;
    crate::actions::vars::validate_variable_name(&req.variable_name)
        .map_err(|msg| AppError::new("repo.actions.invalid_variable_name", msg))?;
    crate::actions::vars::validate_variable_value(&req.value)
        .map_err(|msg| AppError::new("repo.actions.invalid_variable_value", msg))?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    // Reuse the existing row's id so referential identity survives edits.
    let id = ctx
        .db
        .find_action_variable(&accessible.row.id, &req.variable_name)
        .await
        .map_err(db_err)?
        .map(|r| r.id)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    ctx.db
        .upsert_action_variable(&id, &accessible.row.id, &req.variable_name, &req.value)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true, "name": req.variable_name }))
}

/// `repo.actions.variables.delete` — Admin.
pub async fn delete_variable(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionVariablesDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.variables.delete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let ok = ctx
        .db
        .delete_action_variable(&accessible.row.id, &req.variable_name)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": ok }))
}

/// `repo.actions.environments.variablesList` — Admin; values included.
pub async fn list_environment_variables(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionVariablesListResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentVariablesListRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.environments.variablesList input: {e}"),
            )
        })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let env = ctx
        .db
        .find_action_environment(&accessible.row.id, &req.environment)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.actions.env_not_found", "environment not found"))?;
    let rows = ctx
        .db
        .list_action_environment_variables(&env.id)
        .await
        .map_err(db_err)?;
    Ok(ActionVariablesListResponse {
        variables: rows
            .into_iter()
            .map(|r| ActionVariablePublic {
                name: r.name,
                value: r.value,
                updated_at: r.updated_at,
            })
            .collect(),
    })
}

/// `repo.actions.environments.variablesPut` — Admin.
pub async fn put_environment_variable(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentVariablePutRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.environments.variablesPut input: {e}"),
            )
        })?;
    crate::actions::vars::validate_variable_name(&req.variable_name)
        .map_err(|msg| AppError::new("repo.actions.invalid_variable_name", msg))?;
    crate::actions::vars::validate_variable_value(&req.value)
        .map_err(|msg| AppError::new("repo.actions.invalid_variable_value", msg))?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let env = ctx
        .db
        .find_action_environment(&accessible.row.id, &req.environment)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.actions.env_not_found", "environment not found"))?;
    ctx.db
        .upsert_action_environment_variable(
            &Uuid::new_v4().to_string(),
            &env.id,
            &req.variable_name,
            &req.value,
        )
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true, "name": req.variable_name }))
}

/// `repo.actions.environments.variablesDelete` — Admin.
pub async fn delete_environment_variable(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentVariableDeleteRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.environments.variablesDelete input: {e}"),
            )
        })?;
    let accessible = resolve_repo_for_admin(ctx, &req.owner, &req.name).await?;
    let env = ctx
        .db
        .find_action_environment(&accessible.row.id, &req.environment)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("repo.actions.env_not_found", "environment not found"))?;
    let ok = ctx
        .db
        .delete_action_environment_variable(&env.id, &req.variable_name)
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
        required_reviewer_usernames: Vec::new(),
        prevent_self_review: raw
            .get("prevent_self_review")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
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
    let mut environments: Vec<ActionEnvironmentPublic> =
        rows.iter().map(environment_public).collect();
    resolve_reviewer_usernames(ctx, &mut environments).await?;
    Ok(RepoActionsEnvironmentsListResponse { environments })
}

/// Resolve stored reviewer ids to display usernames on a set of public
/// environment records (one batch user lookup).
async fn resolve_reviewer_usernames(
    ctx: &RpcCtx,
    envs: &mut [ActionEnvironmentPublic],
) -> Result<(), AppError> {
    let mut ids: Vec<String> = envs
        .iter()
        .flat_map(|e| e.protection.required_reviewers.iter().cloned())
        .collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Ok(());
    }
    let mut names: HashMap<String, String> = HashMap::new();
    for u in ctx.db.find_users_by_ids(&ids).await.map_err(db_err)? {
        names.insert(u.id, u.username);
    }
    for e in envs.iter_mut() {
        e.protection.required_reviewer_usernames = e
            .protection
            .required_reviewers
            .iter()
            .filter_map(|id| names.get(id).cloned())
            .collect();
    }
    Ok(())
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
    let mut environment = environment_public(&row);
    resolve_reviewer_usernames(ctx, std::slice::from_mut(&mut environment)).await?;
    Ok(RepoActionsEnvironmentResponse { environment })
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
    // Reviewers resolve by user id or username — the UI sends usernames,
    // integrations may carry ids; both normalize to ids before storage so the
    // stored rule is stable across renames.
    let mut reviewers = Vec::with_capacity(req.protection.required_reviewers.len());
    for token in &req.protection.required_reviewers {
        let hit = match ctx.db.find_user_by_id(token).await {
            Ok(Some(_)) => Some(token.clone()),
            _ => match ctx.db.find_user_by_username(token).await {
                Ok(Some(u)) => Some(u.id),
                _ => None,
            },
        };
        match hit {
            Some(uid) => {
                if !reviewers.contains(&uid) {
                    reviewers.push(uid);
                }
            }
            None => {
                return Err(AppError::new(
                    "rpc.bad_input",
                    format!("unknown reviewer: {token}"),
                ))
            }
        }
    }
    // GitHub caps required reviewers at 6 per environment.
    if reviewers.len() > 6 {
        return Err(AppError::new(
            "rpc.bad_input",
            "required_reviewers is capped at 6",
        ));
    }
    let protection_json = serde_json::json!({
        "wait_timer_minutes": req.protection.wait_timer_minutes,
        "required_reviewers": reviewers,
        "prevent_self_review": req.protection.prevent_self_review,
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

/// Resolve the repo + environment row for env-secret ops — the env must
/// exist; secrets against a not-yet-created environment 404.
async fn resolve_env_for_admin(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    environment: &str,
) -> Result<oxidean_db::ActionEnvironmentRow, AppError> {
    let accessible = resolve_repo_for_admin(ctx, owner, name).await?;
    ctx.db
        .find_action_environment(&accessible.row.id, environment)
        .await
        .map_err(db_err)?
        .ok_or_else(|| {
            AppError::new(
                "repo.actions.environment_not_found",
                "environment not found",
            )
        })
}

/// `repo.actions.environments.secretsList` — Admin; names only.
pub async fn list_environment_secrets(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionSecretsListResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentSecretsListRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.environments.secretsList input: {e}"),
            )
        })?;
    let env = resolve_env_for_admin(ctx, &req.owner, &req.name, &req.environment).await?;
    let rows = ctx
        .db
        .list_action_environment_secrets(&env.id)
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

/// `repo.actions.environments.secretsPut` — Admin; value never returned.
pub async fn put_environment_secret(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentSecretPutRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.environments.secretsPut input: {e}"),
            )
        })?;
    validate_secret_name(&req.secret_name)
        .map_err(|msg| AppError::new("repo.actions.invalid_secret_name", msg))?;
    crate::actions::secrets::validate_secret_value(&req.value)
        .map_err(|msg| AppError::new("repo.actions.invalid_secret_value", msg))?;
    let env = resolve_env_for_admin(ctx, &req.owner, &req.name, &req.environment).await?;
    let ciphertext = encrypt_secret(&req.value)
        .map_err(|e| AppError::new("repo.actions.secret_encrypt_failed", e))?;
    ctx.db
        .upsert_action_environment_secret(
            &Uuid::new_v4().to_string(),
            &env.id,
            &req.secret_name,
            &ciphertext,
        )
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true, "name": req.secret_name }))
}

/// `repo.actions.environments.secretsDelete` — Admin.
pub async fn delete_environment_secret(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: RepoActionsEnvironmentSecretDeleteRequest =
        serde_json::from_value(input).map_err(|e| {
            AppError::new(
                "rpc.bad_input",
                format!("invalid repo.actions.environments.secretsDelete input: {e}"),
            )
        })?;
    validate_secret_name(&req.secret_name)
        .map_err(|msg| AppError::new("repo.actions.invalid_secret_name", msg))?;
    let env = resolve_env_for_admin(ctx, &req.owner, &req.name, &req.environment).await?;
    let ok = ctx
        .db
        .delete_action_environment_secret(&env.id, &req.secret_name)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": ok }))
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
    let rows = match req.run_id.as_deref() {
        Some(run_id) => {
            let run = ctx
                .db
                .find_action_run_by_id(run_id)
                .await
                .map_err(db_err)?
                .ok_or_else(not_found)?;
            if run.repository_id != accessible.row.id {
                return Err(not_found());
            }
            ctx.db
                .list_action_deployments_for_run(run_id)
                .await
                .map_err(db_err)?
        }
        // Repo-level listing — powers the Environments view's history column.
        None => ctx
            .db
            .list_action_deployments_for_repo(
                &accessible.row.id,
                req.environment.as_deref(),
                i64::from(req.limit.unwrap_or(100)),
            )
            .await
            .map_err(db_err)?,
    };
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
    let req: ActionJobApproveRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.approveJob input: {e}"),
        )
    })?;
    let job = review_held_job(ctx, &req.owner, &req.name, &req.job_id, "approved", None).await?;
    Ok(ActionJobApproveResponse { job })
}

/// `repo.actions.reviewDeployment` — Write+ (+ reviewer membership); approve
/// or reject an environment-held job with an optional comment. Mirrors
/// GitHub's `POST /actions/runs/{run_id}/pending_deployments`: `rejected`
/// fails the job and its dependents skip via normal `needs:` resolution.
pub async fn review_deployment(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionDeploymentReviewResponse, AppError> {
    let req: ActionDeploymentReviewRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.reviewDeployment input: {e}"),
        )
    })?;
    if !matches!(req.state.as_str(), "approved" | "rejected") {
        return Err(AppError::new(
            "rpc.bad_input",
            "state must be 'approved' or 'rejected'",
        ));
    }
    let job = review_held_job(
        ctx,
        &req.owner,
        &req.name,
        &req.job_id,
        &req.state,
        req.comment,
    )
    .await?;
    Ok(ActionDeploymentReviewResponse { job })
}

/// Shared approve/reject path for held jobs — reviewer-membership and
/// `prevent_self_review` gating, review-row recording, release/fail,
/// scheduler recompute, and wake.
async fn review_held_job(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    job_id: &str,
    state: &str,
    comment: Option<String>,
) -> Result<ActionJobPublic, AppError> {
    let caller = require_verified(ctx).await?;
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    let job = ctx
        .db
        .find_action_job_by_id(job_id)
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
    // Required-reviewer membership + prevent_self_review: the hold is a
    // deployment review only when the env names reviewers — check the stored
    // rule before releasing.
    let mut reviewers: Vec<String> = Vec::new();
    let mut prevent_self_review = false;
    if let Some(env_name) = job.environment_name.as_deref() {
        if let Some(env_row) = ctx
            .db
            .find_action_environment(&accessible.row.id, env_name)
            .await
            .map_err(db_err)?
        {
            let (r, p) = env_protection_parts(&env_row);
            reviewers = r;
            prevent_self_review = p;
        }
    }
    if !reviewers.is_empty()
        && !reviewers.iter().any(|r| r == &caller.id)
        && !meets(accessible.capability, Capability::Admin)
    {
        return Err(AppError::new(
            "repo.actions.not_a_reviewer",
            "caller is not a required reviewer for this environment",
        ));
    }
    if prevent_self_review && run.triggered_by.as_deref() == Some(caller.id.as_str()) {
        return Err(AppError::new(
            "repo.actions.self_review_forbidden",
            "the run's triggering actor may not review this deployment",
        ));
    }
    let resolved = if state == "approved" {
        ctx.db.approve_action_job(&job.id).await.map_err(db_err)?
    } else {
        ctx.db.reject_action_job(&job.id).await.map_err(db_err)?
    };
    if !resolved {
        return Err(AppError::new(
            "repo.actions.job_not_held",
            "job is not waiting for approval",
        ));
    }
    // A rejection is a terminal transition — publish the commit status (which
    // also emits `workflow_job` `completed`) since the job never reaches a
    // runner or the scheduler's terminal paths.
    if state == "rejected" {
        let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
        let _ = crate::actions::statuses::publish_from_job_update(
            &ctx.db,
            &job.id,
            "failure",
            origin.as_deref(),
        )
        .await;
    }
    // Record the decision against environment-held jobs — review history is
    // a deployment-review artifact, so fork-approval holds (no environment)
    // do not accrue rows. After the CAS so a raced approve/reject pair can't
    // both land a recorded review.
    if let Some(env_name) = job.environment_name.as_deref() {
        ctx.db
            .insert_deployment_review(&oxidean_db::actions::ActionDeploymentReviewRow {
                id: Uuid::new_v4().to_string(),
                repository_id: accessible.row.id.clone(),
                run_id: run.id.clone(),
                job_id: job.id.clone(),
                environment: env_name.to_string(),
                reviewer_id: caller.id.clone(),
                reviewer_username: caller.username.clone(),
                state: state.to_string(),
                comment: comment.clone(),
                created_at: String::new(),
            })
            .await
            .map_err(db_err)?;
    }
    if let Err(e) = Box::pin(crate::actions::scheduler::recompute_ready_jobs(
        &ctx.db,
        &job.run_id,
    ))
    .await
    {
        tracing::warn!(error = %e, run_id = %job.run_id, "post-review recompute failed");
    }
    if let Err(e) = ctx.db.recompute_action_run_status(&job.run_id).await {
        tracing::warn!(error = %e, run_id = %job.run_id, "post-review rollup failed");
    }
    crate::actions::task_wake::wake();
    let job = ctx
        .db
        .find_action_job_by_id(job_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    let jobs = ctx
        .db
        .list_action_jobs_for_run(&run.id)
        .await
        .map_err(db_err)?;
    let view = job_view_ctx(ctx, &accessible.row.id, &run, &jobs, accessible.capability).await?;
    Ok(job_public(&job, &view))
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
    let disabled = ctx
        .db
        .list_disabled_action_workflows(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(ActionWorkflowsListResponse {
        workflows: discovered
            .workflows
            .iter()
            .map(|w| ActionWorkflowPublic {
                path: w.path.clone(),
                name: w.document.name.clone(),
                supports_dispatch: w.document.triggers.workflow_dispatch,
                disabled: disabled.contains(&w.path),
                inputs: dispatch_inputs_public(w.document.triggers.dispatch_inputs.as_ref()),
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

/// Serialize `on.workflow_dispatch.inputs` declarations for the dispatch
/// form. Declared shape: `{inputs: {name: {type, required, default,
/// description, options[]}}}` — undeclared fields are ignored (GitHub accepts
/// extras at dispatch time).
fn dispatch_inputs_public(declared: Option<&serde_json::Value>) -> Vec<ActionDispatchInputPublic> {
    let Some(obj) = declared.and_then(|v| v.as_object()) else {
        return Vec::new();
    };
    let mut out: Vec<ActionDispatchInputPublic> = obj
        .iter()
        .map(|(name, spec)| {
            let spec = spec.as_object();
            let get = |k: &str| spec.and_then(|m| m.get(k));
            ActionDispatchInputPublic {
                name: name.clone(),
                input_type: get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("string")
                    .to_string(),
                required: get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                description: get("description")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                default: get("default").cloned(),
                options: get("options")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    // Stable order for the form — YAML maps don't guarantee one.
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// `repo.actions.listArtifacts` — Read+; artifact metadata for a run.
pub async fn list_artifacts(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionArtifactsListResponse, AppError> {
    let req: ActionArtifactsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.listArtifacts input: {e}"),
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
        .list_action_artifacts_for_run(&req.run_id)
        .await
        .map_err(db_err)?;
    Ok(ActionArtifactsListResponse {
        artifacts: rows
            .iter()
            .map(|a| ActionArtifactPublic {
                id: a.id.clone(),
                run_id: a.run_id.clone(),
                job_id: a.job_id.clone(),
                name: a.name.clone(),
                size_bytes: a.size_bytes,
                digest_sha256: a.digest_sha256.clone(),
                expires_at: a.expires_at.clone(),
                created_at: a.created_at.clone(),
            })
            .collect(),
    })
}

/// `repo.actions.deleteArtifact` — Write+; drop the record and payload file.
pub async fn delete_artifact(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionArtifactDeleteResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionArtifactDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.deleteArtifact input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    let artifact = ctx
        .db
        .find_action_artifact_by_id(&req.artifact_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if artifact.repository_id != accessible.row.id {
        return Err(not_found());
    }
    let deleted = ctx
        .db
        .delete_action_artifact(&req.artifact_id)
        .await
        .map_err(db_err)?;
    if deleted {
        let path = ctx.actions_artifacts_dir.join(&artifact.storage_path);
        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, path = %path.display(), "artifact payload removal failed");
            }
        }
    }
    Ok(ActionArtifactDeleteResponse { deleted })
}

/// `repo.actions.listCaches` — Read+; committed cache entries + usage rollup.
pub async fn list_caches(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionCachesListResponse, AppError> {
    let req: ActionCachesListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.listCaches input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Read) {
        return Err(not_found());
    }
    let rows = ctx
        .db
        .list_action_caches_for_repo(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let total_size_bytes = rows.iter().map(|c| c.size_bytes).sum();
    Ok(ActionCachesListResponse {
        caches: rows
            .iter()
            .map(|c| ActionCachePublic {
                id: c.id.clone(),
                key: c.cache_key.clone(),
                version: c.version.clone(),
                scope: c.scope.clone(),
                size_bytes: c.size_bytes,
                created_at: c.created_at.clone(),
                last_accessed_at: c.last_accessed_at.clone(),
            })
            .collect(),
        total_size_bytes,
    })
}

/// `repo.actions.deleteCache` — Write+; drop the record and payload file.
pub async fn delete_cache(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionCacheDeleteResponse, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionCacheDeleteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.deleteCache input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    let cache = ctx
        .db
        .find_action_cache_by_id(&req.cache_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    if cache.repository_id != accessible.row.id {
        return Err(not_found());
    }
    let deleted = ctx
        .db
        .delete_action_cache(&req.cache_id)
        .await
        .map_err(db_err)?;
    if deleted {
        let path = ctx.actions_cache_dir.join(&cache.storage_path);
        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, path = %path.display(), "cache payload removal failed");
            }
        }
    }
    Ok(ActionCacheDeleteResponse { deleted })
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
    if ctx
        .db
        .is_action_workflow_disabled(&accessible.row.id, &wf.path)
        .await
        .map_err(db_err)?
    {
        return Err(AppError::new(
            "repo.actions.workflow_disabled",
            "workflow is disabled",
        ));
    }
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

/// `repo.actions.dispatchRepository` — `repository_dispatch` fan-out for
/// external integrations (GitHub `POST /repos/{o}/{r}/dispatches`). Write+.
/// `event_type` is the activity type (`on.repository_dispatch.types:` filters
/// match it); `client_payload` lands verbatim as `github.event.client_payload`.
pub async fn dispatch_repository(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionDispatchResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: ActionDispatchRepositoryRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.dispatchRepository input: {e}"),
        )
    })?;
    let event_type = req.event_type.trim();
    if event_type.is_empty() || event_type.len() > 100 {
        return Err(AppError::new(
            "rpc.bad_input",
            "event_type is required (1–100 characters)",
        ));
    }
    if let Some(cp) = &req.client_payload {
        if !cp.is_object() {
            return Err(AppError::new(
                "rpc.bad_input",
                "client_payload must be a JSON object",
            ));
        }
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
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let payload = serde_json::json!({
        "action": event_type,
        "client_payload": req.client_payload.unwrap_or(serde_json::json!({})),
        "repository": repository,
        "sender": crate::actions::payload::user_embed(&user.username),
    });
    let Some(ev) = crate::actions::events::rpc_lifecycle_event(
        ctx,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &accessible.row.default_branch,
        "repository_dispatch",
        event_type,
        payload,
        Some(user.id),
    ) else {
        return Err(AppError::new(
            "repo.internal",
            "repository storage is not available",
        ));
    };
    crate::actions::events::notify_lifecycle_actions(&ctx.db, ctx.git.as_ref(), &ev).await;
    Ok(ActionDispatchResponse {
        ok: true,
        run_id: None,
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
    // GitHub caps `on.workflow_dispatch.inputs` at 25 defined inputs.
    if declared.len() > 25 {
        return Err(format!(
            "workflow declares {} inputs — GitHub's maximum is 25",
            declared.len()
        ));
    }
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

/// `repo.actions.setWorkflowEnabled` — Write+; GitHub's disable/enable
/// workflow toggle. Path must name a file under `.github/workflows/`.
pub async fn set_workflow_enabled(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let _ = require_verified(ctx).await?;
    let req: ActionSetWorkflowEnabledRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.actions.setWorkflowEnabled input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    let path = req.path.trim().trim_matches('/');
    let valid = path.starts_with(".github/workflows/")
        && (path.ends_with(".yml") || path.ends_with(".yaml"))
        && !path.contains("..")
        && path.len() < 255;
    if !valid {
        return Err(AppError::new(
            "rpc.bad_input",
            "path must name a .yml/.yaml file under .github/workflows/",
        ));
    }
    ctx.db
        .set_action_workflow_disabled(&accessible.row.id, path, !req.enabled)
        .await
        .map_err(db_err)?;
    Ok(serde_json::json!({ "ok": true, "path": path, "enabled": req.enabled }))
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
    crate::actions::task_wake::wake();
    run_mutation_response(ctx, &accessible, &req.run_id).await
}

/// `repo.actions.rerunRun` — Write+; requeues the run and all its jobs.
pub async fn rerun_run(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<ActionRunMutationResponse, AppError> {
    let (req, accessible, run) = run_mutation_target(ctx, input, "rerunRun").await?;
    // GitHub forbids re-running an in-flight run — only a finished run (or a
    // finished job inside one) can be requeued. Without the guard a zombie
    // runner from the still-open attempt would keep streaming into the new
    // attempt's reset log.
    if run.finished_at.is_none() {
        return Err(AppError::new(
            "repo.actions.run_not_finished",
            "workflow run has not finished",
        ));
    }
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
        if job.finished_at.is_none() {
            return Err(AppError::new(
                "repo.actions.job_not_finished",
                "job has not finished",
            ));
        }
    }
    let reset_jobs = ctx
        .db
        .requeue_action_run(
            &req.run_id,
            req.failed_only.unwrap_or(false),
            req.job_id.as_deref(),
            req.debug.unwrap_or(false),
        )
        .await
        .map_err(db_err)?;
    // Rerun replaces the prior attempt's state: drop each reset job's stored
    // log (plain + compressed-at-rest) so the new attempt starts empty, and
    // revoke the dead attempt's job tokens so a lingering runner cannot
    // append into the new attempt.
    for job_id in &reset_jobs {
        if let Err(e) =
            crate::actions::logs::delete_job_log(&ctx.actions_log_dir, &req.run_id, job_id).await
        {
            tracing::warn!(error = %e, job_id = %job_id, "rerun log wipe failed");
        }
        if let Err(e) = ctx.db.revoke_action_job_token_for_job(job_id).await {
            tracing::warn!(error = %e, job_id = %job_id, "rerun token revoke failed");
        }
    }
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
        // Rerun re-announces each reset job at its new lifecycle state —
        // `queued` for runnable legs, `waiting` for re-gated ones.
        for job in &jobs {
            if !reset_jobs.contains(&job.id) {
                continue;
            }
            let action = if job.status == "waiting" {
                "waiting"
            } else {
                "queued"
            };
            crate::webhook::dispatch::notify_workflow_job(&ctx.db, &job.id, action).await;
        }
    }
    // API-04: a requeued run is a new `workflow_run` `requested` delivery.
    crate::webhook::dispatch::notify_workflow_run(&ctx.db, &req.run_id, "requested", &ctx.env_name)
        .await;
    crate::actions::task_wake::wake();
    run_mutation_response(ctx, &accessible, &req.run_id).await
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
            // `on: workflow_run` fans out on every terminal run — cancel too
            // (GitHub fires `completed` with conclusion `cancelled`).
            crate::actions::events::notify_workflow_run_actions(
                &ctx.db,
                ctx.git.as_ref(),
                &ctx.repos_dir,
                &run,
                &ctx.env_name,
            )
            .await;
        }
    }
    // API-04: cancellation completes the run with conclusion `cancelled`.
    crate::webhook::dispatch::notify_workflow_run(&ctx.db, &req.run_id, "completed", &ctx.env_name)
        .await;
    // Commit statuses: interrupted legs flip pending → error so required
    // checks don't hang on a cancelled run (skipped legs stay green).
    let origin = std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok();
    if let Err(e) =
        crate::actions::statuses::publish_cancelled_for_run(&ctx.db, &req.run_id, origin.as_deref())
            .await
    {
        tracing::warn!(error = %e, run_id = %req.run_id, "failed to publish cancelled statuses");
    }
    run_mutation_response(ctx, &accessible, &req.run_id).await
}

/// `repo.actions.deleteRun` — Write+; deletes a finished run, its jobs,
/// logs, and artifact payloads (GitHub "Delete workflow run"). In-flight
/// runs must be cancelled first — deleting a live run would orphan the
/// runner mid-step.
pub async fn delete_run(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<serde_json::Value, AppError> {
    let (req, _accessible, run) = run_mutation_target(ctx, input, "deleteRun").await?;
    if run.finished_at.is_none() {
        return Err(AppError::new(
            "repo.actions.run_not_finished",
            "cancel the run before deleting it",
        ));
    }
    let files = ctx
        .db
        .delete_action_run(&req.run_id)
        .await
        .map_err(db_err)?
        .unwrap_or_default();
    // Log directory per run + every artifact payload the run owned.
    let log_dir = ctx.actions_log_dir.join(&req.run_id);
    if let Err(e) = tokio::fs::remove_dir_all(&log_dir).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(error = %e, run_id = %req.run_id, "run log dir removal failed");
        }
    }
    for rel in &files.artifact_storage_paths {
        let path = ctx.actions_artifacts_dir.join(rel);
        if let Err(e) = tokio::fs::remove_file(&path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(error = %e, path = %path.display(), "artifact payload removal failed");
            }
        }
    }
    Ok(serde_json::json!({ "ok": true }))
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
    accessible: &crate::repo::AccessibleRepo,
    run_id: &str,
) -> Result<ActionRunMutationResponse, AppError> {
    let run = ctx
        .db
        .find_action_run_by_id(run_id)
        .await
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    debug_assert_eq!(run.repository_id, accessible.row.id);
    let actors = actors_for_runs(ctx, std::slice::from_ref(&run)).await?;
    Ok(ActionRunMutationResponse {
        run: run_public(&run, &actors, accessible.capability),
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

#[cfg(test)]
mod tests {
    use super::resolve_dispatch_inputs;
    use serde_json::json;

    #[test]
    fn dispatch_inputs_resolve_types_and_defaults() {
        let declared = json!({
            "name": {"type": "string"},
            "verbose": {"type": "boolean", "default": false},
            "level": {"type": "choice", "options": ["a", "b"]},
            "count": {"type": "number"},
            "env": {"type": "environment", "required": true},
        });
        // Required without default must be supplied; choice must be in
        // options; boolean/number strings coerce.
        let err = resolve_dispatch_inputs(Some(&declared), None).unwrap_err();
        assert!(err.contains("required"), "{err}");
        let out = resolve_dispatch_inputs(
            Some(&declared),
            Some(&json!({"env": "prod", "level": "a", "count": "3", "extra": "kept"})),
        )
        .unwrap();
        assert_eq!(out["env"], json!("prod"));
        assert_eq!(out["verbose"], json!(false));
        assert_eq!(out["count"], json!(3.0));
        assert_eq!(out["extra"], json!("kept"));
        let err = resolve_dispatch_inputs(
            Some(&declared),
            Some(&json!({"env": "prod", "level": "zzz"})),
        )
        .unwrap_err();
        assert!(err.contains("one of"), "{err}");
    }

    #[test]
    fn dispatch_inputs_cap_25() {
        let declared = json!((0..26)
            .map(|i| (format!("i{i}"), json!({"type": "string"})))
            .collect::<serde_json::Map<String, serde_json::Value>>());
        let err = resolve_dispatch_inputs(Some(&declared), None).unwrap_err();
        assert!(err.contains("25"), "{err}");
        let ok = json!((0..25)
            .map(|i| (format!("i{i}"), json!({"type": "string"})))
            .collect::<serde_json::Map<String, serde_json::Value>>());
        assert!(resolve_dispatch_inputs(Some(&ok), None).is_ok());
    }
}
