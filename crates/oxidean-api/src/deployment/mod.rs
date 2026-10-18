//! Deployment RPC handlers — the public Deployments surface (GitHub
//! REST `/repos/{owner}/{repo}/deployments` parity). Creations and status
//! transitions emit `deployment` / `deployment_status` webhooks and fire
//! matching `on:` Actions triggers (job-token callers stay suppressed per
//! GitHub's GITHUB_TOKEN non-recursion rule).

use oxidean_core::{
    AppError, DeploymentCreateRequest, DeploymentCreateResponse, DeploymentDeleteResponse,
    DeploymentGetResponse, DeploymentListRequest, DeploymentListResponse, DeploymentRefRequest,
    DeploymentStatusCreateRequest, DeploymentStatusListRequest, DeploymentStatusListResponse,
    DeploymentStatusPublic,
};
use oxidean_db::deployments::NewDeploymentStatus;
use oxidean_db::ActionDeploymentRow;
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::repo::{ensure_not_archived, meets, not_found, resolve_repo_for_read, Capability};
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;

const DESCRIPTION_MAX_CHARS: usize = 65_536;
const LIST_LIMIT_CAP: i64 = 100;

/// GitHub `deployment_status.state` vocabulary.
const VALID_STATES: &[&str] = &[
    "error",
    "failure",
    "inactive",
    "in_progress",
    "pending",
    "queued",
    "success",
    "waiting",
];

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("deployment db error: {e}");
        AppError::new("deployment.internal", "deployment operation failed")
    }
}

fn deployment_not_found() -> AppError {
    AppError::new("deployment.not_found", "Deployment not found")
}

fn validate_description(description: Option<&str>) -> Result<String, AppError> {
    let stored = description.unwrap_or("").to_string();
    if stored.chars().count() > DESCRIPTION_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("description exceeds {DESCRIPTION_MAX_CHARS} characters"),
        ));
    }
    Ok(stored)
}

fn deployment_to_public(
    row: &ActionDeploymentRow,
    creator_username: Option<String>,
) -> oxidean_core::ActionDeploymentPublic {
    crate::actions::rpc::deployment_public(row, creator_username)
}

fn status_to_public(
    row: &oxidean_db::ActionDeploymentStatusRow,
    creator_username: Option<String>,
) -> DeploymentStatusPublic {
    DeploymentStatusPublic {
        id: row.id.clone(),
        deployment_id: row.deployment_id.clone(),
        state: row.state.clone(),
        description: row.description.clone(),
        environment: row.environment.clone(),
        environment_url: row.environment_url.clone(),
        target_url: row.target_url.clone(),
        log_url: row.log_url.clone(),
        creator_id: row.creator_id.clone(),
        creator_username,
        created_at: row.created_at.clone(),
    }
}

/// Batch-resolve `creator_id` → username for a deployment page.
async fn deployment_creator_usernames(
    ctx: &RpcCtx,
    rows: &[ActionDeploymentRow],
) -> std::collections::HashMap<String, String> {
    let mut ids: Vec<String> = rows.iter().filter_map(|r| r.creator_id.clone()).collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return std::collections::HashMap::new();
    }
    ctx.db
        .find_users_by_ids(&ids)
        .await
        .map(|users| users.into_iter().map(|u| (u.id, u.username)).collect())
        .unwrap_or_default()
}

async fn load_deployment_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    id: &str,
) -> Result<ActionDeploymentRow, AppError> {
    let row = ctx
        .db
        .find_action_deployment_by_id(id)
        .await
        .map_err(db_err)?
        .ok_or_else(deployment_not_found)?;
    if row.repository_id != repo_id {
        return Err(deployment_not_found());
    }
    Ok(row)
}

/// `deployment.list` — Read+; `sha`/`ref`/`task`/`environment` filter like
/// GitHub's query params.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeploymentListResponse, AppError> {
    let req: DeploymentListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid deployment.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let limit = i64::from(req.limit.unwrap_or(30)).clamp(1, LIST_LIMIT_CAP);
    let rows = ctx
        .db
        .list_action_deployments_filtered(
            &accessible.row.id,
            req.environment.as_deref(),
            req.sha.as_deref(),
            req.git_ref.as_deref(),
            req.task.as_deref(),
            limit,
        )
        .await
        .map_err(db_err)?;
    let names = deployment_creator_usernames(ctx, &rows).await;
    Ok(DeploymentListResponse {
        deployments: rows
            .iter()
            .map(|r| {
                deployment_to_public(
                    r,
                    r.creator_id.as_ref().and_then(|id| names.get(id).cloned()),
                )
            })
            .collect(),
    })
}

/// `deployment.get` — Read+; one deployment by id.
pub async fn get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeploymentGetResponse, AppError> {
    let req: DeploymentRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid deployment.get input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_deployment_in_repo(ctx, &accessible.row.id, &req.id).await?;
    let username = match row.creator_id.as_deref() {
        Some(id) => ctx
            .db
            .find_user_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|u| u.username),
        None => None,
    };
    Ok(deployment_to_public(&row, username))
}

/// `deployment.create` — Write+ (deployments perm). Resolves `ref` against
/// the bare repo, honours GitHub's `auto_merge`, `required_contexts`,
/// `transient_environment` and `production_environment` semantics, then
/// emits `deployment` (created) — webhook + `on: deployment` trigger.
#[allow(clippy::too_many_lines)]
pub async fn create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeploymentCreateResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: DeploymentCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid deployment.create input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;

    let git_ref = req.git_ref.trim().to_string();
    if git_ref.is_empty() {
        return Err(AppError::new("rpc.bad_input", "ref is required"));
    }
    let task = req.task.as_deref().unwrap_or("deploy").trim().to_string();
    if task.is_empty() || task.chars().count() > 255 {
        return Err(AppError::new("rpc.bad_input", "invalid task"));
    }
    let environment = req
        .environment
        .as_deref()
        .unwrap_or("production")
        .trim()
        .to_string();
    if environment.is_empty() || environment.chars().count() > 255 {
        return Err(AppError::new("rpc.bad_input", "invalid environment"));
    }
    let description = validate_description(req.description.as_deref())?;
    let transient_environment = req.transient_environment.unwrap_or(false);

    let bare = crate::git::bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )
    .map_err(|_| not_found())?;

    // Resolve ref → sha. GitHub accepts branch names, tags, `refs/…`, and
    // raw shas; try them in GitHub's order.
    let mut candidates: Vec<String> = Vec::new();
    if git_ref.starts_with("refs/") {
        candidates.push(git_ref.clone());
    } else {
        candidates.push(format!("refs/heads/{git_ref}"));
        candidates.push(format!("refs/tags/{git_ref}"));
        candidates.push(git_ref.clone());
    }
    let mut resolved_ref: Option<(String, String)> = None;
    for cand in &candidates {
        if let Ok(sha) = ctx.git.rev_parse(&bare, cand).await {
            resolved_ref = Some((cand.clone(), sha));
            break;
        }
    }
    let Some((resolved_ref, mut sha)) = resolved_ref else {
        return Err(AppError::new(
            "deployment.ref_not_found",
            format!("No ref found for: {git_ref}"),
        ));
    };

    // `required_contexts`: omitted = verify every context on the requested
    // sha; `[]` skips verification; named contexts must each be `success`.
    // GitHub verifies contexts on the requested commit *before* attempting
    // auto-merge — a contexts failure wins over a merge conflict.
    if req.required_contexts.as_deref() != Some(&[]) {
        let statuses = ctx
            .db
            .list_commit_statuses(&accessible.row.id, &sha)
            .await
            .map_err(db_err)?;
        // Latest status per context wins (statuses append chronologically).
        let mut latest: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
        for s in &statuses {
            latest.insert(s.context.as_str(), s.state.as_str());
        }
        let failed: Vec<String> = match &req.required_contexts {
            None => latest
                .iter()
                .filter(|(_, state)| **state != "success")
                .map(|(ctx_name, _)| (*ctx_name).to_string())
                .collect(),
            Some(required) => required
                .iter()
                .filter(|name| latest.get(name.as_str()).copied() != Some("success"))
                .cloned()
                .collect(),
        };
        if !failed.is_empty() {
            return Err(AppError::new(
                "deployment.required_contexts",
                format!(
                    "Conflict: commit status checks failed for {}",
                    failed.join(", ")
                ),
            ));
        }
    }

    // `auto_merge` (GitHub default true): merge the default branch into the
    // requested ref when it is *behind* — only for branch refs that aren't
    // the default branch. Already-merged refs deploy at their own sha.
    let auto_merge = req.auto_merge.unwrap_or(true);
    let branch_name = resolved_ref.strip_prefix("refs/heads/");
    if auto_merge
        && branch_name.is_some()
        && branch_name != Some(accessible.row.default_branch.as_str())
    {
        if let Ok(default_sha) = ctx
            .git
            .rev_parse(
                &bare,
                &format!("refs/heads/{}", accessible.row.default_branch),
            )
            .await
        {
            let behind = ctx
                .git
                .is_ancestor(&bare, &default_sha, &sha)
                .await
                .map(|contains| !contains)
                .unwrap_or(false);
            if behind {
                let branch = branch_name.unwrap_or(&resolved_ref);
                match ctx
                    .git
                    .merge_commit(
                        &bare,
                        branch,
                        &default_sha,
                        &format!(
                            "Merge branch '{}' into {} (deployment auto-merge)",
                            accessible.row.default_branch, git_ref
                        ),
                    )
                    .await
                {
                    Ok(tip) => sha = tip,
                    Err(e) => {
                        return Err(AppError::new(
                            "deployment.merge_conflict",
                            format!("auto-merge failed: {e}"),
                        ));
                    }
                }
            }
        }
    }

    // `production_environment` defaults from the environment's protection
    // rules (GitHub marks protected environments as production). The env
    // row auto-creates on first reference, matching the job path.
    let env_row = ctx
        .db
        .ensure_action_environment(
            &Uuid::new_v4().to_string(),
            &accessible.row.id,
            &environment,
        )
        .await
        .ok();
    let production_environment = match req.production_environment {
        Some(v) => v,
        None => env_row
            .map(|env| {
                serde_json::from_str::<serde_json::Value>(&env.protection_json)
                    .map(|p| {
                        p.get("required_reviewers")
                            .and_then(|v| v.as_array())
                            .is_some_and(|a| !a.is_empty())
                            || p.get("wait_timer_minutes")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0)
                                > 0
                            || p.get("deployment_branches")
                                .and_then(|v| v.as_array())
                                .is_some_and(|a| !a.is_empty())
                    })
                    .unwrap_or(false)
            })
            .unwrap_or_default(),
    };

    let payload_ser = req
        .payload
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid payload: {e}")))?;
    let deployment_id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_action_deployment(&oxidean_db::deployments::NewDeployment {
            id: &deployment_id,
            repository_id: &accessible.row.id,
            environment: &environment,
            run_id: None,
            job_id: None,
            sha: &sha,
            git_ref: &resolved_ref,
            url: None,
            task: &task,
            description: &description,
            payload: payload_ser.as_deref(),
            creator_id: Some(&user.id),
            transient_environment,
            production_environment,
        })
        .await
        .map_err(db_err)?;

    // Non-transient deployments supersede prior active deployments in the
    // same environment (GitHub marks them `inactive`).
    if !transient_environment || production_environment {
        if let Ok(priors) = ctx
            .db
            .list_active_deployments_for_environment(
                &accessible.row.id,
                &environment,
                &deployment_id,
            )
            .await
        {
            for prior in priors {
                let sid = Uuid::new_v4().to_string();
                dispatch::record_deployment_status_webhook(
                    &ctx.db,
                    &prior,
                    &NewDeploymentStatus {
                        id: &sid,
                        deployment_id: &prior.id,
                        state: "inactive",
                        description: "",
                        environment: Some(&environment),
                        environment_url: None,
                        target_url: None,
                        log_url: None,
                        creator_id: Some(&user.id),
                    },
                    &accessible.row.id,
                    &ctx.env_name,
                )
                .await;
            }
        }
    }

    dispatch::emit_deployment_webhook(&ctx.db, &row, &accessible.row.id, &ctx.env_name).await;
    notify_actions_deployment(ctx, &accessible, &row, &user).await;

    Ok(deployment_to_public(&row, Some(user.username.clone())))
}

/// `deployment.status.list` — Read+; statuses newest-first (GitHub shape).
pub async fn status_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeploymentStatusListResponse, AppError> {
    let req: DeploymentStatusListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid deployment.status.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let _deployment = load_deployment_in_repo(ctx, &accessible.row.id, &req.deployment_id).await?;
    let limit = i64::from(req.limit.unwrap_or(30)).clamp(1, LIST_LIMIT_CAP);
    let rows = ctx
        .db
        .list_action_deployment_statuses(&req.deployment_id, limit)
        .await
        .map_err(db_err)?;
    let mut ids: Vec<String> = rows.iter().filter_map(|r| r.creator_id.clone()).collect();
    ids.sort();
    ids.dedup();
    let names: std::collections::HashMap<String, String> = if ids.is_empty() {
        std::collections::HashMap::new()
    } else {
        ctx.db
            .find_users_by_ids(&ids)
            .await
            .map(|users| users.into_iter().map(|u| (u.id, u.username)).collect())
            .unwrap_or_default()
    };
    Ok(DeploymentStatusListResponse {
        statuses: rows
            .iter()
            .map(|r| {
                status_to_public(
                    r,
                    r.creator_id.as_ref().and_then(|id| names.get(id).cloned()),
                )
            })
            .collect(),
    })
}

/// `deployment.status.create` — Write+; records the status, emits
/// `deployment_status` (webhook + `on:` trigger), and applies GitHub's
/// `auto_inactive` rule on `success`.
#[allow(clippy::too_many_lines)]
pub async fn status_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeploymentStatusPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: DeploymentStatusCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid deployment.status.create input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;

    let state = req.state.trim().to_string();
    if !VALID_STATES.contains(&state.as_str()) {
        return Err(AppError::new(
            "rpc.bad_input",
            format!(
                "invalid state '{state}' (expected one of {})",
                VALID_STATES.join(", ")
            ),
        ));
    }
    let description = validate_description(req.description.as_deref())?;
    let deployment = load_deployment_in_repo(ctx, &accessible.row.id, &req.deployment_id).await?;
    // `target_url` is GitHub's deprecated alias for `log_url`.
    let log_url = req.log_url.as_deref().or(req.target_url.as_deref());

    let status_id = Uuid::new_v4().to_string();
    let row = dispatch::record_deployment_status_webhook(
        &ctx.db,
        &deployment,
        &NewDeploymentStatus {
            id: &status_id,
            deployment_id: &deployment.id,
            state: &state,
            description: &description,
            environment: req.environment.as_deref(),
            environment_url: req.environment_url.as_deref(),
            target_url: req.target_url.as_deref(),
            log_url,
            creator_id: Some(&user.id),
        },
        &accessible.row.id,
        &ctx.env_name,
    )
    .await
    .ok_or_else(|| db_err("deployment status insert failed".into()))?;

    notify_actions_deployment_status(ctx, &accessible, &deployment, &row, &user).await;

    // `auto_inactive` (GitHub default true): a `success` status marks prior
    // deployments to the same environment inactive.
    if state == "success" && req.auto_inactive.unwrap_or(true) {
        if let Ok(priors) = ctx
            .db
            .list_active_deployments_for_environment(
                &accessible.row.id,
                &deployment.environment,
                &deployment.id,
            )
            .await
        {
            for prior in priors {
                let sid = Uuid::new_v4().to_string();
                if let Some(prior_status) = dispatch::record_deployment_status_webhook(
                    &ctx.db,
                    &prior,
                    &NewDeploymentStatus {
                        id: &sid,
                        deployment_id: &prior.id,
                        state: "inactive",
                        description: "",
                        environment: Some(&deployment.environment),
                        environment_url: None,
                        target_url: None,
                        log_url: None,
                        creator_id: Some(&user.id),
                    },
                    &accessible.row.id,
                    &ctx.env_name,
                )
                .await
                {
                    notify_actions_deployment_status(
                        ctx,
                        &accessible,
                        &prior,
                        &prior_status,
                        &user,
                    )
                    .await;
                }
            }
        }
    }

    Ok(status_to_public(&row, Some(user.username.clone())))
}

/// `deployment.delete` — Write+; GitHub requires the deployment be
/// `inactive` first (latest status, when present).
pub async fn delete(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<DeploymentDeleteResponse, AppError> {
    let _user = require_verified(ctx).await?;
    let req: DeploymentRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid deployment.delete input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let row = load_deployment_in_repo(ctx, &accessible.row.id, &req.id).await?;
    if let Some(state) = &row.latest_state {
        if state != "inactive" {
            return Err(AppError::new(
                "deployment.active",
                "Deployment has active statuses and cannot be deleted; mark it inactive first",
            ));
        }
    }
    let ok = ctx
        .db
        .delete_action_deployment(&row.id)
        .await
        .map_err(db_err)?;
    Ok(DeploymentDeleteResponse { ok })
}

/// `on: deployment` lifecycle dispatch (created) — soft-fails like every
/// lifecycle fan-out.
async fn notify_actions_deployment(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    row: &ActionDeploymentRow,
    user: &oxidean_db::UserRow,
) {
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let payload =
        crate::actions::payload::deployment_payload(row, &user.username, &repository, &sender);
    let Some(event) = crate::actions::events::rpc_lifecycle_event(
        ctx,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &accessible.row.default_branch,
        "deployment",
        "created",
        payload,
        Some(user.id.clone()),
    ) else {
        return;
    };
    crate::actions::events::notify_lifecycle_actions(&ctx.db, ctx.git.as_ref(), &event).await;
}

/// `on: deployment_status` lifecycle dispatch (created).
async fn notify_actions_deployment_status(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    deployment: &ActionDeploymentRow,
    status: &oxidean_db::ActionDeploymentStatusRow,
    user: &oxidean_db::UserRow,
) {
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let deployment_creator = match deployment.creator_id.as_deref() {
        Some(id) => ctx
            .db
            .find_user_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_default(),
        None => String::new(),
    };
    let payload = crate::actions::payload::deployment_status_payload(
        status,
        deployment,
        &user.username,
        &deployment_creator,
        &repository,
        &sender,
    );
    let Some(event) = crate::actions::events::rpc_lifecycle_event(
        ctx,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &accessible.row.default_branch,
        "deployment_status",
        "created",
        payload,
        Some(user.id.clone()),
    ) else {
        return;
    };
    crate::actions::events::notify_lifecycle_actions(&ctx.db, ctx.git.as_ref(), &event).await;
}
