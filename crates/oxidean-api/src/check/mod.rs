//! Checks RPC handlers — the public Checks surface (GitHub REST
//! `/repos/{owner}/{repo}/check-runs` + `/check-suites` parity). Creations
//! and transitions emit `check_run` / `check_suite` webhooks and fire
//! matching `on:` Actions triggers (job-token callers stay suppressed per
//! GitHub's GITHUB_TOKEN non-recursion rule). Actions-run jobs project a
//! `github-actions` suite with one run per job; API-created runs share the
//! `api` suite per commit.

use oxidean_core::{
    AppError, CheckRunAnnotationInput, CheckRunAnnotationPublic, CheckRunAnnotationsListRequest,
    CheckRunAnnotationsListResponse, CheckRunCreateRequest, CheckRunCreateResponse,
    CheckRunGetResponse, CheckRunListForRefRequest, CheckRunListForSuiteRequest,
    CheckRunListResponse, CheckRunPublic, CheckRunRefRequest, CheckRunRerequestResponse,
    CheckRunUpdateRequest, CheckRunUpdateResponse, CheckSuiteCreateRequest,
    CheckSuiteCreateResponse, CheckSuiteGetResponse, CheckSuiteListRequest, CheckSuiteListResponse,
    CheckSuiteRefRequest, CheckSuiteRerequestResponse,
};
use oxidean_db::{CheckRunRow, CheckSuiteRow, Database};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::repo::{ensure_not_archived, meets, not_found, resolve_repo_for_read, Capability};
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;

const LIST_LIMIT_CAP: i64 = 100;
/// GitHub caps inline annotations at 50 per create/update call.
const ANNOTATIONS_PER_CALL: usize = 50;
const NAME_MAX_CHARS: usize = 255;
const OUTPUT_FIELD_MAX_CHARS: usize = 65_536;

/// GitHub `check_run.status` values a caller may write.
const VALID_RUN_STATUSES: &[&str] = &[
    "queued",
    "in_progress",
    "completed",
    "waiting",
    "pending",
    "requested",
];

/// GitHub `check_run.conclusion` vocabulary.
const VALID_CONCLUSIONS: &[&str] = &[
    "success",
    "failure",
    "neutral",
    "cancelled",
    "skipped",
    "timed_out",
    "action_required",
    "startup_failure",
    "stale",
];

/// GitHub `annotation_level` vocabulary.
const VALID_ANNOTATION_LEVELS: &[&str] = &["notice", "warning", "failure"];

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("check db error: {e}");
        AppError::new("check.internal", "check operation failed")
    }
}

fn suite_not_found() -> AppError {
    AppError::new("check.suite_not_found", "Check suite not found")
}

fn run_not_found() -> AppError {
    AppError::new("check.run_not_found", "Check run not found")
}

fn validate_status(status: &str) -> Result<(), AppError> {
    if !VALID_RUN_STATUSES.contains(&status) {
        return Err(AppError::new(
            "rpc.bad_input",
            format!(
                "invalid status '{status}' (expected one of {})",
                VALID_RUN_STATUSES.join(", ")
            ),
        ));
    }
    Ok(())
}

fn validate_conclusion(conclusion: Option<&str>, status: &str) -> Result<(), AppError> {
    match conclusion {
        Some(c) if !VALID_CONCLUSIONS.contains(&c) => Err(AppError::new(
            "rpc.bad_input",
            format!(
                "invalid conclusion '{c}' (expected one of {})",
                VALID_CONCLUSIONS.join(", ")
            ),
        )),
        Some(_) if status != "completed" => Err(AppError::new(
            "rpc.bad_input",
            "conclusion is only valid when status is 'completed'",
        )),
        None if status == "completed" => Err(AppError::new(
            "rpc.bad_input",
            "conclusion is required when status is 'completed'",
        )),
        _ => Ok(()),
    }
}

fn validate_annotations(annotations: &[CheckRunAnnotationInput]) -> Result<(), AppError> {
    if annotations.len() > ANNOTATIONS_PER_CALL {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("a maximum of {ANNOTATIONS_PER_CALL} annotations are allowed per call"),
        ));
    }
    for a in annotations {
        if a.path.trim().is_empty() {
            return Err(AppError::new(
                "rpc.bad_input",
                "annotation path is required",
            ));
        }
        if a.start_line < 1 || a.end_line < a.start_line {
            return Err(AppError::new(
                "rpc.bad_input",
                format!(
                    "annotation line range {}-{} is invalid for {}",
                    a.start_line, a.end_line, a.path
                ),
            ));
        }
        if !VALID_ANNOTATION_LEVELS.contains(&a.annotation_level.as_str()) {
            return Err(AppError::new(
                "rpc.bad_input",
                format!(
                    "invalid annotation_level '{}' (expected one of {})",
                    a.annotation_level,
                    VALID_ANNOTATION_LEVELS.join(", ")
                ),
            ));
        }
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<String, AppError> {
    let name = name.trim().to_string();
    if name.is_empty() || name.chars().count() > NAME_MAX_CHARS {
        return Err(AppError::new("rpc.bad_input", "invalid check name"));
    }
    Ok(name)
}

fn validate_output(output: &oxidean_core::CheckRunOutput) -> Result<(), AppError> {
    for (field, value) in [
        ("title", &output.title),
        ("summary", &output.summary),
        ("text", &output.text),
    ] {
        if let Some(v) = value {
            if v.chars().count() > OUTPUT_FIELD_MAX_CHARS {
                return Err(AppError::new(
                    "rpc.bad_input",
                    format!("output.{field} exceeds {OUTPUT_FIELD_MAX_CHARS} characters"),
                ));
            }
        }
    }
    validate_annotations(&output.annotations)
}

fn suite_to_public(
    row: &CheckSuiteRow,
    creator_username: Option<String>,
) -> oxidean_core::CheckSuitePublic {
    oxidean_core::CheckSuitePublic {
        id: row.id.clone(),
        head_sha: row.head_sha.clone(),
        head_branch: row.head_branch.clone(),
        app: row.app.clone(),
        run_id: row.run_id.clone(),
        status: row.status.clone(),
        conclusion: row.conclusion.clone(),
        url: row.url.clone(),
        creator_id: row.creator_id.clone(),
        creator_username,
        check_runs_count: row.check_runs_count,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

fn run_to_public(
    row: &CheckRunRow,
    app: &str,
    creator_username: Option<String>,
    annotations_count: i64,
) -> CheckRunPublic {
    CheckRunPublic {
        id: row.id.clone(),
        name: row.name.clone(),
        head_sha: row.head_sha.clone(),
        status: row.status.clone(),
        conclusion: row.conclusion.clone(),
        details_url: row.details_url.clone(),
        external_id: row.external_id.clone(),
        check_suite_id: row.check_suite_id.clone(),
        app: app.to_string(),
        job_id: row.job_id.clone(),
        run_id: row.run_id.clone(),
        output: oxidean_core::CheckRunOutput {
            title: row.output_title.clone(),
            summary: row.output_summary.clone(),
            text: row.output_text.clone(),
            annotations: Vec::new(),
            annotations_count,
        },
        creator_id: row.creator_id.clone(),
        creator_username,
        started_at: row.started_at.clone(),
        completed_at: row.completed_at.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

fn annotation_to_public(
    row: &oxidean_db::CheckRunAnnotationRow,
    owner: &str,
    repo: &str,
    head_sha: &str,
) -> CheckRunAnnotationPublic {
    CheckRunAnnotationPublic {
        id: row.id.clone(),
        check_run_id: row.check_run_id.clone(),
        path: row.path.clone(),
        blob_href: Some(format!(
            "/{owner}/{repo}/blob/{head_sha}/{}#L{}",
            row.path, row.start_line
        )),
        start_line: row.start_line,
        end_line: row.end_line,
        start_column: row.start_column,
        end_column: row.end_column,
        annotation_level: row.annotation_level.clone(),
        message: row.message.clone(),
        title: row.title.clone(),
        raw_details: row.raw_details.clone(),
        created_at: row.created_at.clone(),
    }
}

/// Batch `creator_id` → username for a page of rows (suites or runs share
/// the same `creator_id` column).
async fn creator_usernames(
    ctx: &RpcCtx,
    ids: impl Iterator<Item = Option<String>>,
) -> std::collections::HashMap<String, String> {
    let mut ids: Vec<String> = ids.flatten().collect();
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

async fn annotations_count_map(
    ctx: &RpcCtx,
    runs: &[CheckRunRow],
) -> std::collections::HashMap<String, i64> {
    let mut map = std::collections::HashMap::new();
    for r in runs {
        let n = annotation_count(&ctx.db, r).await;
        map.insert(r.id.clone(), n);
    }
    map
}

/// Total annotations visible on a check run — stored
/// `check_run_annotations` plus, for Actions-bound runs, the job's
/// log-extracted `action_annotations` (GitHub shows workflow-command
/// annotations on the check).
pub async fn annotation_count(db: &Database, run: &CheckRunRow) -> i64 {
    let stored = db
        .list_check_run_annotations(&run.id)
        .await
        .map(|a| a.len() as i64)
        .unwrap_or(0);
    let job_notes = match &run.job_id {
        Some(job_id) => db
            .list_action_annotations_for_job(job_id)
            .await
            .map(|a| a.len() as i64)
            .unwrap_or(0),
        None => 0,
    };
    stored + job_notes
}

async fn load_suite_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    id: &str,
) -> Result<CheckSuiteRow, AppError> {
    let row = ctx
        .db
        .find_check_suite(id)
        .await
        .map_err(db_err)?
        .ok_or_else(suite_not_found)?;
    if row.repository_id != repo_id {
        return Err(suite_not_found());
    }
    Ok(row)
}

async fn load_run_in_repo(ctx: &RpcCtx, repo_id: &str, id: &str) -> Result<CheckRunRow, AppError> {
    let row = ctx
        .db
        .find_check_run(id)
        .await
        .map_err(db_err)?
        .ok_or_else(run_not_found)?;
    if row.repository_id != repo_id {
        return Err(run_not_found());
    }
    Ok(row)
}

/// Resolve a `ref`/`sha` input to a commit sha against the bare repo —
/// GitHub's candidate order (`refs/heads`, `refs/tags`, verbatim).
async fn resolve_sha(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    git_ref: &str,
) -> Result<String, AppError> {
    let bare = crate::git::bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )
    .map_err(|_| not_found())?;
    let mut candidates: Vec<String> = Vec::new();
    if git_ref.starts_with("refs/") {
        candidates.push(git_ref.to_string());
    } else {
        candidates.push(format!("refs/heads/{git_ref}"));
        candidates.push(format!("refs/tags/{git_ref}"));
        candidates.push(git_ref.to_string());
    }
    for cand in &candidates {
        if let Ok(sha) = ctx.git.rev_parse(&bare, cand).await {
            if !sha.is_empty() {
                return Ok(sha);
            }
        }
    }
    Err(AppError::new(
        "check.ref_not_found",
        format!("No ref found for: {git_ref}"),
    ))
}

/// `check.suites.list` — Read+; suites newest-first, `ref`/`head_sha` narrow.
pub async fn suites_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckSuiteListResponse, AppError> {
    let req: CheckSuiteListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.suites.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let limit = i64::from(req.limit.unwrap_or(30)).clamp(1, LIST_LIMIT_CAP);
    let head_sha = match (&req.git_ref, &req.head_sha) {
        (Some(r), _) => Some(resolve_sha(ctx, &accessible, r.trim()).await?),
        (None, sha) => sha.clone(),
    };
    let rows = ctx
        .db
        .list_check_suites(&accessible.row.id, head_sha.as_deref(), limit)
        .await
        .map_err(db_err)?;
    let names = creator_usernames(ctx, rows.iter().map(|r| r.creator_id.clone())).await;
    Ok(CheckSuiteListResponse {
        suites: rows
            .iter()
            .map(|r| {
                suite_to_public(
                    r,
                    r.creator_id.as_ref().and_then(|id| names.get(id).cloned()),
                )
            })
            .collect(),
    })
}

/// `check.suites.get` — Read+; one suite by id.
pub async fn suites_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckSuiteGetResponse, AppError> {
    let req: CheckSuiteRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.suites.get input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_suite_in_repo(ctx, &accessible.row.id, &req.id).await?;
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
    Ok(suite_to_public(&row, username))
}

/// `check.suites.create` — Write+; a suite under the `api` app label.
/// Emits `check_suite` (`requested`) — webhook + `on:` trigger.
pub async fn suites_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckSuiteCreateResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CheckSuiteCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.suites.create input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let sha = resolve_sha(ctx, &accessible, req.head_sha.trim()).await?;

    let suite_id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_check_suite(&oxidean_db::checks::NewCheckSuite {
            id: &suite_id,
            repository_id: &accessible.row.id,
            head_sha: &sha,
            head_branch: None,
            app: "api",
            run_id: None,
            status: "queued",
            conclusion: None,
            url: None,
            creator_id: Some(&user.id),
        })
        .await
        .map_err(db_err)?;

    let env_name = ctx.env_name.clone();
    dispatch::emit_check_suite_webhook(&ctx.db, &row, "requested", &env_name).await;
    notify_actions_check_suite(ctx, &accessible, &row, "requested", &user).await;

    Ok(suite_to_public(&row, Some(user.username.clone())))
}

/// `check.suites.rerequest` — Write+; asks the app to re-run the suite.
/// State is unchanged (the app decides); GitHub emits `rerequested`.
pub async fn suites_rerequest(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckSuiteRerequestResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CheckSuiteRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.suites.rerequest input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let row = load_suite_in_repo(ctx, &accessible.row.id, &req.id).await?;
    if row.run_id.is_some() {
        return Err(AppError::new(
            "check.suite_actions_bound",
            "Rerequesting a GitHub Actions check suite is not supported; rerun the workflow instead",
        ));
    }
    let env_name = ctx.env_name.clone();
    dispatch::emit_check_suite_webhook(&ctx.db, &row, "rerequested", &env_name).await;
    notify_actions_check_suite(ctx, &accessible, &row, "rerequested", &user).await;
    Ok(suite_to_public(&row, Some(user.username.clone())))
}

/// `check.runs.listForSuite` — Read+; `check_name`/`status` filter like
/// GitHub's query params.
pub async fn runs_list_for_suite(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckRunListResponse, AppError> {
    let req: CheckRunListForSuiteRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.runs.listForSuite input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let suite = load_suite_in_repo(ctx, &accessible.row.id, &req.suite_id).await?;
    let limit = i64::from(req.limit.unwrap_or(30)).clamp(1, LIST_LIMIT_CAP);
    let mut rows = ctx
        .db
        .list_check_runs_for_suite(&suite.id, limit)
        .await
        .map_err(db_err)?;
    if let Some(name) = req.check_name.as_deref() {
        rows.retain(|r| r.name == name);
    }
    if let Some(status) = req.status.as_deref() {
        rows.retain(|r| r.status == status);
    }
    let names = creator_usernames(ctx, rows.iter().map(|r| r.creator_id.clone())).await;
    let counts = annotations_count_map(ctx, &rows).await;
    Ok(CheckRunListResponse {
        check_runs: rows
            .iter()
            .map(|r| {
                run_to_public(
                    r,
                    &suite.app,
                    r.creator_id.as_ref().and_then(|id| names.get(id).cloned()),
                    counts.get(&r.id).copied().unwrap_or(0),
                )
            })
            .collect(),
    })
}

/// `check.runs.listForRef` — Read+; resolves `ref` → sha then filters
/// `check_name`/`status`/`conclusion`.
pub async fn runs_list_for_ref(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckRunListResponse, AppError> {
    let req: CheckRunListForRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.runs.listForRef input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let sha = resolve_sha(ctx, &accessible, req.git_ref.trim()).await?;
    let limit = i64::from(req.limit.unwrap_or(30)).clamp(1, LIST_LIMIT_CAP);
    let rows = ctx
        .db
        .list_check_runs_for_sha(
            &accessible.row.id,
            &sha,
            req.check_name.as_deref(),
            req.status.as_deref(),
            req.conclusion.as_deref(),
            limit,
        )
        .await
        .map_err(db_err)?;
    // Runs carry the app label of their suite — resolve per-suite apps.
    let mut suite_apps: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    for r in &rows {
        if let std::collections::hash_map::Entry::Vacant(e) =
            suite_apps.entry(r.check_suite_id.clone())
        {
            let app = ctx
                .db
                .find_check_suite(&r.check_suite_id)
                .await
                .ok()
                .flatten()
                .map(|s| s.app)
                .unwrap_or_else(|| "api".into());
            e.insert(app);
        }
    }
    let names = creator_usernames(ctx, rows.iter().map(|r| r.creator_id.clone())).await;
    let counts = annotations_count_map(ctx, &rows).await;
    Ok(CheckRunListResponse {
        check_runs: rows
            .iter()
            .map(|r| {
                run_to_public(
                    r,
                    suite_apps
                        .get(&r.check_suite_id)
                        .map(String::as_str)
                        .unwrap_or("api"),
                    r.creator_id.as_ref().and_then(|id| names.get(id).cloned()),
                    counts.get(&r.id).copied().unwrap_or(0),
                )
            })
            .collect(),
    })
}

/// `check.runs.get` — Read+; one run by id.
pub async fn runs_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckRunGetResponse, AppError> {
    let req: CheckRunRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.runs.get input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_run_in_repo(ctx, &accessible.row.id, &req.id).await?;
    let app = ctx
        .db
        .find_check_suite(&row.check_suite_id)
        .await
        .ok()
        .flatten()
        .map(|s| s.app)
        .unwrap_or_else(|| "api".into());
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
    let count = annotation_count(&ctx.db, &row).await;
    Ok(run_to_public(&row, &app, username, count))
}

/// `check.runs.create` — Write+; runs land under the shared `api` suite for
/// the commit (GitHub per-app suite semantics). Emits `check_run`
/// (`created`) and, for a fresh suite, `check_suite` (`requested`).
#[allow(clippy::too_many_lines)]
pub async fn runs_create(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckRunCreateResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CheckRunCreateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.runs.create input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;

    let check_name = validate_name(&req.check_name)?;
    let sha = resolve_sha(ctx, &accessible, req.head_sha.trim()).await?;
    let status = req.status.as_deref().unwrap_or("queued").trim().to_string();
    validate_status(&status)?;
    validate_conclusion(req.conclusion.as_deref(), &status)?;
    if let Some(output) = &req.output {
        validate_output(output)?;
    }

    // The shared `api` suite per commit (GitHub's per-app suite).
    let env_name = ctx.env_name.clone();
    let (suite, suite_created) = match ctx
        .db
        .find_check_suite_for(&accessible.row.id, &sha, "api")
        .await
        .map_err(db_err)?
    {
        Some(s) => (s, false),
        None => {
            let suite_id = Uuid::new_v4().to_string();
            let s = ctx
                .db
                .insert_check_suite(&oxidean_db::checks::NewCheckSuite {
                    id: &suite_id,
                    repository_id: &accessible.row.id,
                    head_sha: &sha,
                    head_branch: None,
                    app: "api",
                    run_id: None,
                    status: "queued",
                    conclusion: None,
                    url: None,
                    creator_id: Some(&user.id),
                })
                .await
                .map_err(db_err)?;
            (s, true)
        }
    };

    let run_id = Uuid::new_v4().to_string();
    let output = req.output.unwrap_or_default();
    let row = ctx
        .db
        .insert_check_run(&oxidean_db::checks::NewCheckRun {
            id: &run_id,
            repository_id: &accessible.row.id,
            check_suite_id: &suite.id,
            name: &check_name,
            head_sha: &sha,
            status: &status,
            conclusion: req.conclusion.as_deref(),
            details_url: req.details_url.as_deref(),
            external_id: req.external_id.as_deref(),
            output_title: output.title.as_deref(),
            output_summary: output.summary.as_deref(),
            output_text: output.text.as_deref(),
            job_id: None,
            run_id: None,
            creator_id: Some(&user.id),
            started_at: req.started_at.as_deref(),
            completed_at: req.completed_at.as_deref(),
        })
        .await
        .map_err(db_err)?;

    insert_annotations(ctx, &accessible.row.id, &run_id, &output.annotations).await?;
    roll_suite(ctx, &suite.id).await?;

    if suite_created {
        dispatch::emit_check_suite_webhook(&ctx.db, &suite, "requested", &env_name).await;
        notify_actions_check_suite(ctx, &accessible, &suite, "requested", &user).await;
    }
    dispatch::emit_check_run_webhook(&ctx.db, &row, "created", &env_name).await;
    notify_actions_check_run(ctx, &accessible, &row, "created", &user).await;
    // A run born completed also emits `completed` (GitHub emits both).
    if status == "completed" {
        dispatch::emit_check_run_webhook(&ctx.db, &row, "completed", &env_name).await;
        notify_actions_check_run(ctx, &accessible, &row, "completed", &user).await;
        emit_suite_completed_if_due(ctx, &suite.id, &env_name, &accessible, &user).await;
    }

    let count = output.annotations.len() as i64;
    Ok(run_to_public(
        &row,
        &suite.app,
        Some(user.username.clone()),
        count,
    ))
}

/// `check.runs.update` — Write+; omitted fields keep their stored values.
/// Reaching `completed` emits `check_run` (`completed`); new inline
/// annotations append to the stored set.
#[allow(clippy::too_many_lines)]
pub async fn runs_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckRunUpdateResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CheckRunUpdateRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.runs.update input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let row = load_run_in_repo(ctx, &accessible.row.id, &req.id).await?;
    if row.job_id.is_some() {
        return Err(AppError::new(
            "check.run_actions_bound",
            "Actions-bound check runs are owned by the workflow; update the job instead",
        ));
    }

    let next_status = req.status.as_deref().unwrap_or(row.status.as_str());
    validate_status(next_status)?;
    let next_conclusion = match &req.conclusion {
        Some(c) => c.as_deref(),
        None => row.conclusion.as_deref(),
    };
    validate_conclusion(next_conclusion, next_status)?;
    if let Some(name) = &req.check_name {
        validate_name(name)?;
    }
    if let Some(output) = &req.output {
        validate_output(output)?;
    }

    let output = req.output.unwrap_or_default();
    let stored = ctx
        .db
        .update_check_run(
            &row.id,
            &oxidean_db::checks::CheckRunPatch {
                name: req.check_name.as_deref(),
                status: req.status.as_deref(),
                conclusion: req.conclusion.as_ref().map(|c| c.as_deref()),
                details_url: req.details_url.as_ref().map(|v| v.as_deref()),
                external_id: req.external_id.as_ref().map(|v| v.as_deref()),
                output_title: output.title.as_ref().map(|v| Some(v.as_str())),
                output_summary: output.summary.as_ref().map(|v| Some(v.as_str())),
                output_text: output.text.as_ref().map(|v| Some(v.as_str())),
                started_at: req.started_at.as_ref().map(|v| v.as_deref()),
                completed_at: req.completed_at.as_ref().map(|v| v.as_deref()),
            },
        )
        .await
        .map_err(db_err)?;

    if !output.annotations.is_empty() {
        insert_annotations(ctx, &accessible.row.id, &row.id, &output.annotations).await?;
    }
    roll_suite(ctx, &row.check_suite_id).await?;

    let env_name = ctx.env_name.clone();
    if stored.status == "completed" && row.status != "completed" {
        dispatch::emit_check_run_webhook(&ctx.db, &stored, "completed", &env_name).await;
        notify_actions_check_run(ctx, &accessible, &stored, "completed", &user).await;
        emit_suite_completed_if_due(ctx, &row.check_suite_id, &env_name, &accessible, &user).await;
    }

    let count = annotation_count(&ctx.db, &stored).await;
    let app = ctx
        .db
        .find_check_suite(&stored.check_suite_id)
        .await
        .ok()
        .flatten()
        .map(|s| s.app)
        .unwrap_or_else(|| "api".into());
    Ok(run_to_public(
        &stored,
        &app,
        Some(user.username.clone()),
        count,
    ))
}

/// `check.runs.rerequest` — Write+; asks the app to re-run the check.
/// State is unchanged (the app decides); GitHub emits `rerequested`.
pub async fn runs_rerequest(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckRunRerequestResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: CheckRunRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.runs.rerequest input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    if !meets(accessible.capability, Capability::Write) {
        return Err(not_found());
    }
    ensure_not_archived(&accessible)?;
    let row = load_run_in_repo(ctx, &accessible.row.id, &req.id).await?;
    if row.job_id.is_some() {
        return Err(AppError::new(
            "check.run_actions_bound",
            "Rerequesting an Actions-bound check run is not supported; rerun the job instead",
        ));
    }
    let env_name = ctx.env_name.clone();
    dispatch::emit_check_run_webhook(&ctx.db, &row, "rerequested", &env_name).await;
    notify_actions_check_run(ctx, &accessible, &row, "rerequested", &user).await;
    let app = ctx
        .db
        .find_check_suite(&row.check_suite_id)
        .await
        .ok()
        .flatten()
        .map(|s| s.app)
        .unwrap_or_else(|| "api".into());
    let count = annotation_count(&ctx.db, &row).await;
    Ok(run_to_public(
        &row,
        &app,
        Some(user.username.clone()),
        count,
    ))
}

/// `check.annotations.list` — Read+; annotations on one run, insertion order.
pub async fn annotations_list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<CheckRunAnnotationsListResponse, AppError> {
    let req: CheckRunAnnotationsListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid check.annotations.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_run_in_repo(ctx, &accessible.row.id, &req.check_run_id).await?;
    let rows = ctx
        .db
        .list_check_run_annotations(&row.id)
        .await
        .map_err(db_err)?;
    let mut annotations: Vec<CheckRunAnnotationPublic> = rows
        .iter()
        .map(|r| {
            annotation_to_public(
                r,
                &accessible.owner_username,
                &accessible.row.name,
                &row.head_sha,
            )
        })
        .collect();
    // Actions-bound runs also surface the job's log-extracted workflow
    // annotations (`::error::`/`::warning::`/`::notice::`) — GitHub shows
    // them on the check. `error` maps to GitHub's `failure` level.
    if let Some(job_id) = &row.job_id {
        if let Ok(job_notes) = ctx.db.list_action_annotations_for_job(job_id).await {
            for a in job_notes {
                let level = match a.annotation_level.as_str() {
                    "error" => "failure",
                    "warning" => "warning",
                    _ => "notice",
                };
                let path = a.path.unwrap_or_default();
                annotations.push(CheckRunAnnotationPublic {
                    id: a.id,
                    check_run_id: row.id.clone(),
                    blob_href: if path.is_empty() {
                        None
                    } else {
                        Some(format!(
                            "/{}/{}/blob/{}/{}#L{}",
                            accessible.owner_username,
                            accessible.row.name,
                            row.head_sha,
                            path,
                            a.start_line.unwrap_or(1)
                        ))
                    },
                    path,
                    start_line: a.start_line.unwrap_or(1),
                    end_line: a.end_line.unwrap_or(1),
                    start_column: a.start_column,
                    end_column: a.end_column,
                    annotation_level: level.into(),
                    message: a.message,
                    title: a.title,
                    raw_details: None,
                    created_at: a.created_at,
                });
            }
        }
    }
    Ok(CheckRunAnnotationsListResponse { annotations })
}

/// Recompute a suite from its runs (best-effort — warn on failure).
async fn roll_suite(ctx: &RpcCtx, suite_id: &str) -> Result<(), AppError> {
    ctx.db
        .recompute_check_suite(suite_id)
        .await
        .map_err(db_err)?;
    Ok(())
}

/// After a run completes, a suite rolling up to `completed` emits the
/// `check_suite` (`completed`) webhook + lifecycle event.
async fn emit_suite_completed_if_due(
    ctx: &RpcCtx,
    suite_id: &str,
    env_name: &str,
    accessible: &crate::repo::AccessibleRepo,
    user: &oxidean_db::UserRow,
) {
    let Ok(Some(suite)) = ctx.db.find_check_suite(suite_id).await else {
        return;
    };
    if suite.status != "completed" {
        return;
    }
    dispatch::emit_check_suite_webhook(&ctx.db, &suite, "completed", env_name).await;
    notify_actions_check_suite(ctx, accessible, &suite, "completed", user).await;
}

/// Persist inline annotations under a check run.
async fn insert_annotations(
    ctx: &RpcCtx,
    repo_id: &str,
    check_run_id: &str,
    annotations: &[CheckRunAnnotationInput],
) -> Result<(), AppError> {
    for a in annotations {
        let id = Uuid::new_v4().to_string();
        ctx.db
            .insert_check_run_annotation(&oxidean_db::checks::NewCheckRunAnnotation {
                id: &id,
                check_run_id,
                repository_id: repo_id,
                path: a.path.trim(),
                start_line: a.start_line,
                end_line: a.end_line,
                start_column: a.start_column,
                end_column: a.end_column,
                annotation_level: &a.annotation_level,
                message: &a.message,
                title: a.title.as_deref(),
                raw_details: a.raw_details.as_deref(),
            })
            .await
            .map_err(db_err)?;
    }
    Ok(())
}

/// `on: check_run` lifecycle dispatch — soft-fails like every lifecycle
/// fan-out.
async fn notify_actions_check_run(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    row: &CheckRunRow,
    action: &str,
    user: &oxidean_db::UserRow,
) {
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let suite_app = ctx
        .db
        .find_check_suite(&row.check_suite_id)
        .await
        .ok()
        .flatten()
        .map(|s| s.app)
        .unwrap_or_else(|| "api".into());
    let annotations_count = annotation_count(&ctx.db, row).await;
    let payload = crate::actions::payload::check_run_payload(
        action,
        row,
        &suite_app,
        &user.username,
        annotations_count,
        &repository,
        &sender,
    );
    let Some(event) = crate::actions::events::rpc_lifecycle_event(
        ctx,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &accessible.row.default_branch,
        "check_run",
        action,
        payload,
        Some(user.id.clone()),
    ) else {
        return;
    };
    crate::actions::events::notify_lifecycle_actions(&ctx.db, ctx.git.as_ref(), &event).await;
}

/// `on: check_suite` lifecycle dispatch.
async fn notify_actions_check_suite(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    row: &CheckSuiteRow,
    action: &str,
    user: &oxidean_db::UserRow,
) {
    let repository = crate::actions::payload::repository_embed(
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.default_branch,
    );
    let sender = crate::actions::payload::user_embed(&user.username);
    let payload = crate::actions::payload::check_suite_payload(
        action,
        row,
        &user.username,
        &repository,
        &sender,
    );
    let Some(event) = crate::actions::events::rpc_lifecycle_event(
        ctx,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        &accessible.row.default_branch,
        "check_suite",
        action,
        payload,
        Some(user.id.clone()),
    ) else {
        return;
    };
    crate::actions::events::notify_lifecycle_actions(&ctx.db, ctx.git.as_ref(), &event).await;
}
