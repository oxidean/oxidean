//! `repo.actions.*` session RPC (ACT-03 / D-ACT-12 / D-ACT-13 / D-ACT-18 / ACT-06).

use std::collections::HashMap;

use uuid::Uuid;

use oxidean_core::{
    ActionDispatchRequest, ActionDispatchResponse, ActionEnabledRequest, ActionEnabledResponse,
    ActionJobLogRequest, ActionJobLogResponse, ActionJobPublic, ActionListRunnersResponse,
    ActionRegistrationTokenResponse, ActionRunGetRequest, ActionRunGetResponse,
    ActionRunMutationRequest, ActionRunMutationResponse, ActionRunPublic, ActionRunnerPublic,
    ActionRunsListRequest, ActionRunsListResponse, ActionSecretDeleteRequest,
    ActionSecretMetaPublic, ActionSecretPutRequest, ActionSecretsListRequest,
    ActionSecretsListResponse, ActionSetEnabledRequest, ActionWorkflowPublic,
    ActionWorkflowsListRequest, ActionWorkflowsListResponse, AppError,
};
use oxidean_db::{ActionJobRow, ActionRunFilter, ActionRunRow, ActionRunnerRow};

use crate::actions::dispatch::{bare_repo_path, enqueue_run};
use crate::actions::logs::read_job_log;
use crate::actions::secrets::{encrypt_secret, validate_secret_name};
use crate::actions::tokens::mint_registration_token;
use crate::actions::discover_workflows;
use crate::auth::admin::require_admin as require_sys_admin;
use crate::auth::gate::require_verified;
use crate::repo::{
    meets, not_found, resolve_repo_for_admin, resolve_repo_for_read, Capability,
};
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
        status: r.status.clone(),
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
    let ids: Vec<String> = runs
        .iter()
        .filter_map(|r| r.triggered_by.clone())
        .collect();
    let mut out = HashMap::new();
    if ids.is_empty() {
        return Ok(out);
    }
    let mut unique = ids;
    unique.sort();
    unique.dedup();
    for u in ctx
        .db
        .find_users_by_ids(&unique)
        .await
        .map_err(db_err)?
    {
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
        status: j.status.clone(),
        runs_on,
        started_at: j.started_at.clone(),
        finished_at: j.finished_at.clone(),
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
    let nonempty = |v: Option<String>| {
        v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    };
    let branch = nonempty(req.branch).map(|b| {
        b.strip_prefix("refs/heads/")
            .map(str::to_string)
            .unwrap_or(b)
    });
    let mut triggered_by = None;
    if let Some(actor) = nonempty(req.actor) {
        match ctx
            .db
            .find_user_by_username(&actor)
            .await
            .map_err(db_err)?
        {
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
                .list_action_runs_for_repo(
                    &accessible.row.id,
                    &filter,
                    i64::from(per_page),
                    offset,
                )
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
    let treeish = resolve_treeish(ctx, &accessible.row.default_branch, req.git_ref.as_deref(), &bare)
        .await?;
    let discovered = discover_workflows(ctx.git.as_ref(), &bare, &treeish)
        .await
        .map_err(|e| {
            tracing::warn!(error = %e, "workflow discovery failed");
            AppError::new("repo.actions.discovery_failed", "failed to read workflows")
        })?;
    Ok(ActionWorkflowsListResponse {
        workflows: discovered
            .iter()
            .map(|w| ActionWorkflowPublic {
                path: w.path.clone(),
                name: w.document.name.clone(),
                supports_dispatch: w.document.triggers.workflow_dispatch,
            })
            .collect(),
        git_ref: req.git_ref.unwrap_or_else(|| accessible.row.default_branch.clone()),
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
        .iter()
        .find(|w| w.path == wanted || w.document.name == wanted)
        .ok_or_else(|| AppError::new("repo.actions.workflow_not_found", "workflow not found"))?;
    if !wf.document.triggers.workflow_dispatch {
        return Err(AppError::new(
            "repo.actions.dispatch_unsupported",
            "workflow does not declare the workflow_dispatch trigger",
        ));
    }
    let head_ref = ref_display(&req.git_ref, &accessible.row.default_branch);
    let (run_id, _jobs) = enqueue_run(
        &ctx.db,
        &accessible.row.id,
        &wf.path,
        &wf.document,
        "workflow_dispatch",
        &treeish,
        &head_ref,
        Some(&user.id),
    )
    .await
    .map_err(db_err)?;
    Ok(ActionDispatchResponse {
        ok: true,
        run_id: Some(run_id),
    })
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
    // API-04: a requeued run is a new `workflow_run` `requested` delivery.
    crate::webhook::dispatch::notify_workflow_run(
        &ctx.db,
        &req.run_id,
        "requested",
        &ctx.env_name,
    )
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
    crate::webhook::dispatch::notify_workflow_run(
        &ctx.db,
        &req.run_id,
        "completed",
        &ctx.env_name,
    )
    .await;
    run_mutation_response(ctx, &accessible.row.id, &req.run_id).await
}

async fn run_mutation_target(
    ctx: &RpcCtx,
    input: serde_json::Value,
    proc: &str,
) -> Result<(ActionRunMutationRequest, crate::repo::AccessibleRepo, ActionRunRow), AppError> {
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
async fn resolve_treeish(
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
    ctx.git
        .rev_parse(bare, &candidate)
        .await
        .map_err(|e| {
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
