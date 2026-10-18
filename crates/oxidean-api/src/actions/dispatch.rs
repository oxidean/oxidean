//! Async push → workflow enqueue (D-ACT-04 / D-ACT-05). Never fails the git push.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use oxidean_db::Database;
use oxidean_git::GitBackend;
use uuid::Uuid;

use crate::actions::discover_workflows;
use crate::actions::parse::WorkflowDocument;

/// Instance gate from AppState / env (D-ACT-06).
pub fn instance_actions_enabled(flag: bool) -> bool {
    flag
}

/// Parse `OXIDEAN_ACTIONS_ENABLED` (default true). Used when `AppState` is unavailable (SSH/RPC).
pub fn env_actions_enabled() -> bool {
    std::env::var("OXIDEAN_ACTIONS_ENABLED")
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        })
        .unwrap_or(true)
}

#[allow(clippy::too_many_arguments)]
/// Dispatch Actions evaluation after a successful receive-pack.
///
/// Errors are logged only — callers must not fail the push (D-ACT-05 / T-19-09).
pub async fn notify_push_actions(
    db: &Database,
    git: Arc<dyn GitBackend>,
    repos_dir: &Path,
    repository_id: &str,
    owner_slug: &str,
    repo_name: &str,
    pusher_id: Option<&str>,
    updates: &[(String, String, String)],
    actions_enabled_instance: bool,
) {
    if let Err(e) = dispatch_push_inner(
        db,
        git.as_ref(),
        repos_dir,
        repository_id,
        owner_slug,
        repo_name,
        pusher_id,
        updates,
        actions_enabled_instance,
    )
    .await
    {
        tracing::warn!(
            error = %e,
            repository_id,
            "actions push dispatch failed (push already succeeded)"
        );
    }
}

/// GitHub's "skip CI" markers — a head-commit message containing any of
/// these suppresses `push`, `pull_request`, and `pull_request_target`
/// runs. Compared case-insensitively (GitHub's matcher is documented by
/// behaviour, not spec — treat as best-effort).
pub fn message_has_skip_marker(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    [
        "[skip ci]",
        "[ci skip]",
        "[no ci]",
        "[skip actions]",
        "[actions skip]",
    ]
    .iter()
    .any(|k| m.contains(k))
}

#[allow(clippy::too_many_arguments)]
async fn dispatch_push_inner(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    repository_id: &str,
    owner_slug: &str,
    repo_name: &str,
    pusher_id: Option<&str>,
    updates: &[(String, String, String)],
    actions_enabled_instance: bool,
) -> Result<(), String> {
    if !actions_enabled_instance {
        return Ok(());
    }
    if !db
        .get_repo_actions_enabled(repository_id)
        .await
        .unwrap_or(true)
    {
        return Ok(());
    }

    let bare = repos_dir.join(owner_slug).join(format!("{repo_name}.git"));
    if !bare.exists() {
        return Err(format!("bare repo missing: {}", bare.display()));
    }

    let sender = match pusher_id {
        Some(uid) => db
            .find_user_by_id(uid)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_default(),
        None => String::new(),
    };
    let default_branch = db
        .find_repository_by_id(repository_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.default_branch)
        .unwrap_or_else(|| "main".into());
    let repository =
        crate::actions::payload::repository_embed(owner_slug, repo_name, &default_branch);
    let sender_embed = crate::actions::payload::user_embed(&sender);

    // GitHub dispatches one run per updated ref — iterate every non-delete
    // branch/tag update rather than collapsing to a single tip.
    //
    // A push updating more than three tags produces no tag events at all
    // (GitHub's tag-flood guard); branch updates in the same push still run.
    let tag_flood = updates
        .iter()
        .filter(|(_, after, r)| {
            r.starts_with("refs/tags/") && !after.is_empty() && !after.chars().all(|c| c == '0')
        })
        .count()
        > 3;
    for (before, after, refname) in updates {
        if after.chars().all(|c| c == '0') || after.is_empty() {
            continue; // ref deletion never runs workflows
        }
        let (branch, tag) = if let Some(b) = refname.strip_prefix("refs/heads/") {
            (Some(b), None)
        } else if let Some(t) = refname.strip_prefix("refs/tags/") {
            if tag_flood {
                continue;
            }
            (None, Some(t))
        } else {
            continue; // notes/replace/etc — no push workflows on non-branch/tag refs
        };
        // [skip ci]-family markers on the head commit suppress the event.
        if let Ok(c) = git.show_commit(&bare, after, None, None).await {
            if message_has_skip_marker(&format!("{}\n{}", c.subject, c.body)) {
                tracing::info!(
                    repository_id,
                    refname,
                    "actions push skipped — head commit carries a skip marker"
                );
                continue;
            }
        }
        let commits = match git.push_event_commits(&bare, before, after, 2048).await {
            Ok(c) => Some(c),
            Err(e) => {
                // Force-push with unreachable `before`, etc. — read as
                // unconstrained rather than dropping `paths`-filtered runs.
                tracing::warn!(
                    error = %e, repository_id, refname,
                    "push paths filter skipped — commit listing failed"
                );
                None
            }
        };
        let changed: Option<Vec<String>> = commits.as_ref().map(|cs| {
            let mut v: Vec<String> = cs
                .iter()
                .flat_map(|c| {
                    c.added
                        .iter()
                        .chain(c.removed.iter())
                        .chain(c.modified.iter())
                        .cloned()
                })
                .collect();
            v.sort();
            v.dedup();
            v
        });
        // `before` all-zeros (ref creation) or unlistable history → GitHub
        // skips path checks.
        let changed: Option<Vec<String>> = if before.chars().all(|c| c == '0') || before.is_empty()
        {
            None
        } else {
            changed
        };

        let payload_commits: Vec<crate::actions::payload::PayloadCommit> = commits
            .unwrap_or_default()
            .iter()
            .map(|c| crate::actions::payload::PayloadCommit {
                id: c.sha.clone(),
                message: c.subject.clone(),
                author_name: c.author_name.clone(),
                author_email: c.author_email.clone(),
                timestamp: c.authored_at.clone(),
                added: c.added.clone(),
                removed: c.removed.clone(),
                modified: c.modified.clone(),
            })
            .collect();
        let payload = crate::actions::payload::push_payload(
            before,
            after,
            refname,
            &payload_commits,
            if sender.is_empty() {
                None
            } else {
                Some(&sender)
            },
            &repository,
            &sender_embed,
        );

        let discovered = discover_workflows(git, &bare, after)
            .await
            .map_err(|e| e.to_string())?;
        if !discovered.errors.is_empty() {
            for fe in &discovered.errors {
                tracing::warn!(path = %fe.path, error = %fe.message, "invalid workflow file");
            }
            fail_discovery_runs(
                db,
                repository_id,
                &discovered.errors,
                "push",
                after,
                refname,
                pusher_id,
                None,
                Some(&payload),
            )
            .await?;
        }

        let disabled = db.list_disabled_action_workflows(repository_id).await?;
        for wf in &discovered.workflows {
            if !wf.document.triggers.push || disabled.contains(&wf.path) {
                continue;
            }
            if !crate::actions::filters::event_matches(
                &wf.document.triggers.push_filters,
                branch,
                tag,
                changed.as_deref(),
                None,
            ) {
                continue;
            }
            enqueue_run(
                db,
                repository_id,
                &wf.path,
                &wf.document,
                "push",
                after,
                refname,
                pusher_id,
                None,
                Some(&payload),
                false,
            )
            .await?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
/// Create a queued run + jobs from a parsed workflow (no step execution).
pub async fn enqueue_run(
    db: &Database,
    repository_id: &str,
    workflow_path: &str,
    doc: &WorkflowDocument,
    event: &str,
    head_sha: &str,
    head_ref: &str,
    triggered_by: Option<&str>,
    head_repo_id: Option<&str>,
    event_payload: Option<&serde_json::Value>,
    // Fork runs held for maintainer approval — live legs insert with
    // `approval_hold` set (unclaimable, publicly `waiting_approval`) until
    // `repo.actions.approveRun` clears the flag.
    hold_for_approval: bool,
) -> Result<(String, Vec<String>), String> {
    let run_id = Uuid::new_v4().to_string();
    // Enqueue-time expression context (Phase 2): github/env/vars/inputs.
    let prov = crate::actions::eval_ctx::Provenance {
        event,
        head_sha,
        head_ref,
        triggered_by,
        run_id: &run_id,
    };
    let (mut ctx, mut eval_warnings) =
        crate::actions::eval_ctx::build_context(db, repository_id, doc, &prov, event_payload).await;
    // Repository "Workflow permissions" default — `read` restricted |
    // `write` permissive — resolves the scope set when a workflow/job
    // declares no `permissions:` block.
    let default_permissions = db
        .get_repo_actions_default_permissions(repository_id)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, repository_id, "default permissions lookup failed — read");
            "read".into()
        });
    // `name:` interpolates — the run's display name and `github.workflow`.
    let mut name_notes = Vec::new();
    let workflow_name = crate::actions::eval_ctx::interp_or_raw(&doc.name, &ctx, &mut name_notes);
    eval_warnings.extend(name_notes);
    crate::actions::eval_ctx::set_workflow_name(&mut ctx, &workflow_name);
    // `run-name:` — per-run title evaluated against the same enqueue ctx
    // (github/inputs/vars). Empty or absent falls back to the workflow name,
    // matching GitHub's title column.
    let run_title = doc
        .run_name
        .as_deref()
        .map(|s| {
            let mut notes = Vec::new();
            let t = crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut notes);
            eval_warnings.extend(notes);
            t
        })
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| workflow_name.clone());
    db.insert_action_run(
        &run_id,
        repository_id,
        workflow_path,
        &workflow_name,
        event,
        head_sha,
        head_ref,
        &run_title,
        triggered_by,
        head_repo_id,
        event_payload
            .and_then(|v| serde_json::to_string(v).ok())
            .as_deref(),
    )
    .await?;
    // Workflow `concurrency:` — recorded on the run; enforcement lands with
    // the scheduler phase.
    if let Some(group_src) = doc.concurrency.as_ref().and_then(|c| c.group.as_deref()) {
        let mut notes = Vec::new();
        // GitHub concurrency groups are case-insensitive — store lowercase.
        let group =
            crate::actions::eval_ctx::interp_or_raw(group_src, &ctx, &mut notes).to_lowercase();
        eval_warnings.extend(notes);
        if let Err(e) = db.set_action_run_concurrency(&run_id, &group).await {
            tracing::warn!(error = %e, run_id, "failed to record run concurrency group");
        }
    }
    // Fail loudly: a workflow that names features we can't execute — or
    // violates the instance action-source policy (Phase 8) — becomes a
    // failed validation run instead of silently mis-executing.
    let policy_violations =
        crate::actions::policy::check_doc(doc, &crate::actions::policy::policy_from_env());
    if !doc.unsupported.is_empty() || !policy_violations.is_empty() {
        let msg = [
            (!doc.unsupported.is_empty()).then(|| {
                format!(
                    "unsupported workflow features: {}",
                    doc.unsupported.join(", ")
                )
            }),
            (!policy_violations.is_empty())
                .then(|| format!("action source policy: {}", policy_violations.join("; "))),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" | ");
        let job_id = Uuid::new_v4().to_string();
        db.insert_action_job(oxidean_db::JobInsert::basic(
            &job_id,
            &run_id,
            "__validation",
            &msg,
            "[]",
        ))
        .await?;
        if let Err(e) = crate::actions::logs::append_job_log(
            &crate::actions::logs::resolve_log_dir(),
            &run_id,
            &job_id,
            format!("::error::{msg}\n").as_bytes(),
        )
        .await
        {
            tracing::warn!(error = %e, run_id, "failed to write validation log");
        }
        db.update_action_job_status(&job_id, "failure").await?;
        if let Err(e) = db.recompute_action_run_status(&run_id).await {
            tracing::warn!(error = %e, run_id, "validation run rollup failed");
        }
        let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
        crate::webhook::dispatch::notify_workflow_run(db, &run_id, "requested", &env_name).await;
        if let Err(e) = crate::actions::statuses::upsert_job_status(
            db,
            repository_id,
            head_sha,
            &doc.name,
            "__validation",
            "failure",
            &msg,
            None,
            triggered_by,
        )
        .await
        {
            tracing::warn!(error = %e, run_id, "failed to publish validation status");
        }
        return Ok((run_id, vec![job_id]));
    }
    let all_warnings: Vec<String> = doc.warnings.iter().cloned().chain(eval_warnings).collect();
    let log_dir = if all_warnings.is_empty() {
        None
    } else {
        Some(crate::actions::logs::resolve_log_dir())
    };
    let run_supersede = doc
        .concurrency
        .as_ref()
        .and_then(|c| {
            let cip = c
                .cancel_in_progress
                .as_deref()
                .and_then(|s| oxidean_actions_expr::eval_bool(s, &ctx).ok())
                .unwrap_or(false);
            cip.then_some(c.group.as_deref()).flatten()
        })
        .map(|s| crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut Vec::new()).to_lowercase());
    let mut emit = EmitCtx {
        db,
        repository_id,
        run_id: &run_id,
        workflow_path,
        doc,
        head_sha,
        head_ref,
        default_permissions: &default_permissions,
        workflow_name: &workflow_name,
        ctx,
        all_warnings,
        log_dir,
        hold_for_approval,
        supersede_groups: Vec::new(),
        caller: None,
    };
    let (job_ids, job_keys) = emit_jobs(&mut emit, &doc.jobs).await?;
    let supersede_groups = emit.supersede_groups;
    // `concurrency.cancel-in-progress` — workflow group supersedes whole
    // still-running same-group runs; job groups supersede same-group jobs.
    // Affected runners see the cancel on their next update_task/update_log.
    if let Some(group) = &run_supersede {
        match db
            .cancel_superseded_concurrency_runs(repository_id, group, &run_id)
            .await
        {
            Ok(ids) if !ids.is_empty() => {
                tracing::info!(
                    run_id,
                    group,
                    superseded = ids.len(),
                    "concurrency superseded runs"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, run_id, group, "concurrency supersede failed");
            }
            _ => {}
        }
    }
    for group in &supersede_groups {
        match db
            .cancel_superseded_concurrency_jobs(repository_id, group, &run_id)
            .await
        {
            Ok(ids) if !ids.is_empty() => {
                for id in &ids {
                    if let Err(e) = db.recompute_action_run_status(id).await {
                        tracing::warn!(error = %e, run_id = id, "post-supersede rollup failed");
                    }
                }
                tracing::info!(
                    run_id,
                    group,
                    superseded = ids.len(),
                    "concurrency superseded job runs"
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, run_id, group, "concurrency job supersede failed");
            }
            _ => {}
        }
    }
    // Approval gate: set the hold flag on the run — jobs were already
    // inserted held, and the recompute below skips held `waiting` legs
    // (the public `waiting_approval` status derives from the flag).
    if hold_for_approval {
        if let Err(e) = db.hold_run_for_approval(&run_id).await {
            tracing::warn!(error = %e, run_id, "approval hold failed — run stays queued");
        }
    }
    // Needs-jobs whose upstreams were already terminal at enqueue (e.g. a
    // job skipped by `if: false`) resolve immediately.
    if let Err(e) = Box::pin(crate::actions::scheduler::recompute_ready_jobs(db, &run_id)).await {
        tracing::warn!(error = %e, run_id, "post-enqueue ready-job recompute failed");
    }
    // Jobs skipped or failed at enqueue are already terminal — roll the run
    // up immediately so all-skipped workflows finish `success`.
    if let Err(e) = db.recompute_action_run_status(&run_id).await {
        tracing::warn!(error = %e, run_id, "post-enqueue run rollup failed");
    }
    if let Err(e) = crate::actions::statuses::publish_queued_for_run(
        db,
        repository_id,
        head_sha,
        &doc.name,
        &run_id,
        &job_keys,
        std::env::var("OXIDEAN_PUBLIC_ORIGIN").ok().as_deref(),
        None,
        None,
        triggered_by,
    )
    .await
    {
        tracing::warn!(error = %e, run_id = %run_id, "failed to publish queued commit statuses");
    }
    // API-04: `workflow_run` `requested`. `enqueue_run` is shared by push / PR /
    // workflow_dispatch callers that do not all thread `AppState::env_name`, so
    // resolve the same `OXIDEAN_ENV` source the app state was built from.
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    crate::webhook::dispatch::notify_workflow_run(db, &run_id, "requested", &env_name).await;
    // New claimable work — wake `fetch_task` long-polls (Phase 9).
    crate::actions::task_wake::wake();
    Ok((run_id, job_ids))
}

#[allow(clippy::too_many_arguments)]
/// Test/helper path: enqueue from bare path + SHA without git receive-pack.
pub async fn dispatch_push_for_sha(
    db: &Database,
    git: &dyn GitBackend,
    bare: &Path,
    repository_id: &str,
    head_sha: &str,
    head_ref: &str,
    pusher_id: Option<&str>,
    actions_enabled_instance: bool,
) -> Result<usize, String> {
    if !actions_enabled_instance {
        return Ok(0);
    }
    if !db.get_repo_actions_enabled(repository_id).await? {
        return Ok(0);
    }
    // Same skip-marker contract as the post-receive path.
    if let Ok(c) = git.show_commit(bare, head_sha, None, None).await {
        if message_has_skip_marker(&format!("{}\n{}", c.subject, c.body)) {
            return Ok(0);
        }
    }
    let mut n = 0;
    let discovered = discover_workflows(git, bare, head_sha)
        .await
        .map_err(|e| e.to_string())?;
    if !discovered.errors.is_empty() {
        for fe in &discovered.errors {
            tracing::warn!(path = %fe.path, error = %fe.message, "invalid workflow file");
        }
        n += fail_discovery_runs(
            db,
            repository_id,
            &discovered.errors,
            "push",
            head_sha,
            head_ref,
            pusher_id,
            None,
            None,
        )
        .await?;
    }
    // Helper path has no `before` — synthesize a minimal push payload so
    // `github.event`/`OXIDEAN_EVENT_PATH` still carry the ref+head shape.
    let (branch, tag) = if let Some(b) = head_ref.strip_prefix("refs/heads/") {
        (Some(b), None)
    } else if let Some(t) = head_ref.strip_prefix("refs/tags/") {
        (None, Some(t))
    } else {
        (None, None)
    };
    let commits = git
        .push_event_commits(bare, "", head_sha, 2048)
        .await
        .unwrap_or_default();
    let repo_row = db.find_repository_by_id(repository_id).await.ok().flatten();
    let repository = match &repo_row {
        Some(r) => {
            let owner = crate::repo::owner_ref_for_repo(db, r)
                .await
                .ok()
                .flatten()
                .map(|o| o.slug().to_string())
                .unwrap_or_default();
            crate::actions::payload::repository_embed(&owner, &r.name, &r.default_branch)
        }
        None => serde_json::json!({}),
    };
    let payload_commits: Vec<crate::actions::payload::PayloadCommit> = commits
        .iter()
        .map(|c| crate::actions::payload::PayloadCommit {
            id: c.sha.clone(),
            message: c.subject.clone(),
            author_name: c.author_name.clone(),
            author_email: c.author_email.clone(),
            timestamp: c.authored_at.clone(),
            added: c.added.clone(),
            removed: c.removed.clone(),
            modified: c.modified.clone(),
        })
        .collect();
    let payload = crate::actions::payload::push_payload(
        "",
        head_sha,
        head_ref,
        &payload_commits,
        None,
        &repository,
        &serde_json::json!({}),
    );
    let disabled = db.list_disabled_action_workflows(repository_id).await?;
    for wf in &discovered.workflows {
        if !wf.document.triggers.push || disabled.contains(&wf.path) {
            continue;
        }
        // No `before` → changed set unknown → paths filters unconstrained
        // (same as ref-creation semantics).
        if !crate::actions::filters::event_matches(
            &wf.document.triggers.push_filters,
            branch,
            tag,
            None,
            None,
        ) {
            continue;
        }
        enqueue_run(
            db,
            repository_id,
            &wf.path,
            &wf.document,
            "push",
            head_sha,
            head_ref,
            pusher_id,
            None,
            Some(&payload),
            false,
        )
        .await?;
        n += 1;
    }
    Ok(n)
}

/// Phase 8: persist discovery-time file errors as failed `__validation`
/// runs — an unparseable workflow file is a visible failure on the Actions
/// surface (GitHub renders it the same way), not a silent warn-and-skip.
/// Returns how many validation runs were created.
#[allow(clippy::too_many_arguments)]
pub async fn fail_discovery_runs(
    db: &Database,
    repository_id: &str,
    errors: &[crate::actions::DiscoveryFileError],
    event: &str,
    head_sha: &str,
    head_ref: &str,
    triggered_by: Option<&str>,
    head_repo_id: Option<&str>,
    event_payload: Option<&serde_json::Value>,
) -> Result<usize, String> {
    let env_name = std::env::var("OXIDEAN_ENV").unwrap_or_else(|_| "development".into());
    let payload_json = event_payload.and_then(|v| serde_json::to_string(v).ok());
    let mut n = 0;
    for fe in errors {
        let run_id = Uuid::new_v4().to_string();
        db.insert_action_run(
            &run_id,
            repository_id,
            &fe.path,
            &fe.path,
            event,
            head_sha,
            head_ref,
            &format!("Invalid workflow file: {}", fe.path),
            triggered_by,
            head_repo_id,
            payload_json.as_deref(),
        )
        .await?;
        let job_id = Uuid::new_v4().to_string();
        let msg = format!("invalid workflow file: {}", fe.message);
        db.insert_action_job(oxidean_db::JobInsert::basic(
            &job_id,
            &run_id,
            "__validation",
            &msg,
            "[]",
        ))
        .await?;
        if let Err(e) = crate::actions::logs::append_job_log(
            &crate::actions::logs::resolve_log_dir(),
            &run_id,
            &job_id,
            format!("::error::{msg}\n").as_bytes(),
        )
        .await
        {
            tracing::warn!(error = %e, run_id, "failed to write validation log");
        }
        db.update_action_job_status(&job_id, "failure").await?;
        if let Err(e) = db.recompute_action_run_status(&run_id).await {
            tracing::warn!(error = %e, run_id, "validation run rollup failed");
        }
        crate::webhook::dispatch::notify_workflow_run(db, &run_id, "requested", &env_name).await;
        if let Err(e) = crate::actions::statuses::upsert_job_status(
            db,
            repository_id,
            head_sha,
            &fe.path,
            "__validation",
            "failure",
            &msg,
            None,
            triggered_by,
        )
        .await
        {
            tracing::warn!(error = %e, run_id, "failed to publish validation status");
        }
        n += 1;
    }
    Ok(n)
}

pub fn bare_repo_path(repos_dir: &Path, owner: &str, name: &str) -> PathBuf {
    repos_dir.join(owner).join(format!("{name}.git"))
}

/// Run-scoped inputs the per-job emit loop shares — extracted so the
/// reusable-workflow expansion (P13) reuses the identical emission path for
/// callee jobs under a `caller` linkage instead of duplicating ~300 lines.
pub(crate) struct EmitCtx<'a> {
    pub db: &'a Database,
    pub repository_id: &'a str,
    pub run_id: &'a str,
    pub workflow_path: &'a str,
    pub doc: &'a WorkflowDocument,
    pub head_sha: &'a str,
    pub head_ref: &'a str,
    pub default_permissions: &'a str,
    pub workflow_name: &'a str,
    /// Enqueue/expand-time expression context (github/env/vars/inputs).
    pub ctx: oxidean_actions_expr::Context,
    pub all_warnings: Vec<String>,
    pub log_dir: Option<PathBuf>,
    pub hold_for_approval: bool,
    pub supersede_groups: Vec<String>,
    /// Set when emitting callee jobs for a reusable-workflow call — every
    /// emitted row links to the caller leg and carries the call's resolved
    /// inputs/secrets scope on its payload.
    pub caller: Option<CallerEmit>,
}

/// Callee-emission linkage — what `call` metadata lands on each emitted
/// callee payload row.
#[derive(Debug, Clone)]
pub(crate) struct CallerEmit {
    /// The calling job's row id — callee `caller_job_id`.
    pub caller_job_id: String,
    /// Resolved call inputs → callee `github.event.inputs`/`inputs` ctx.
    pub inputs: serde_json::Value,
    /// Secrets forwarding decision — applied at claim when callee secrets
    /// are decrypted (`inherit` passes the caller scope verbatim, a map
    /// forwards each callee name from the caller scope, absent = none).
    pub secrets_json: serde_json::Value,
}

/// Emit one job row per `JobSpec` × matrix leg. Returns
/// `(job_ids, job_keys)`. Callee emission passes `emit.caller = Some(..)` so
/// rows link and `needs`/fail-fast scope within the call.
pub(crate) async fn emit_jobs(
    emit: &mut EmitCtx<'_>,
    jobs: &[crate::actions::parse::JobSpec],
) -> Result<(Vec<String>, Vec<String>), String> {
    let db = emit.db;
    let run_id = emit.run_id;
    let repository_id = emit.repository_id;
    let workflow_path = emit.workflow_path;
    let doc = emit.doc;
    let head_sha = emit.head_sha;
    let head_ref = emit.head_ref;
    let workflow_name = emit.workflow_name;
    let ctx = emit.ctx.clone();
    let log_dir = emit.log_dir.clone();
    let hold_for_approval = emit.hold_for_approval;
    let mut job_ids = Vec::new();
    let mut job_keys = Vec::new();
    for job in jobs {
        // Per-job expression evaluation. Errors in `if`/`runs-on` fail the
        // job loudly; errors in display fields degrade to ::warning::.
        let mut notes: Vec<String> = Vec::new();
        let mut job_status = "queued";
        let mut job_err: Option<String> = None;
        // `jobs.<id>.if` — eval error fails the job; false skips it.
        // Jobs with `needs:` DEFER if-evaluation to ready-time — the
        // `needs.<id>.result/outputs` context doesn't exist at enqueue.
        // Calling jobs (`uses:`) always defer: expansion happens in the
        // scheduler where the call ctx (`with:`/`secrets:` exprs) sees real
        // needs/matrix/inputs.
        let is_call = job.uses.is_some();
        let has_needs = !job.needs.is_empty();
        if is_call || has_needs {
            job_status = "waiting";
        } else if let Some(src) = &job.if_expr {
            match oxidean_actions_expr::eval_bool(src, &ctx) {
                Ok(true) => {}
                Ok(false) => {
                    job_status = "skipped";
                    notes.push(format!("job '{}' skipped — if evaluated to false", job.id));
                }
                Err(e) => job_err = Some(format!("if '{src}' evaluation failed: {e}")),
            }
        }
        // `strategy.matrix` → one row per leg (shared job_key, distinct
        // matrix_key); expansion errors fail the whole job loudly. Calling
        // jobs fan out the call — each leg expands its own call instance.
        let legs = match crate::actions::matrix::expand_legs(job.strategy.as_ref(), &ctx) {
            Ok(l) => l,
            Err(e) => {
                job_err = Some(format!("strategy.matrix: {e}"));
                vec![crate::actions::matrix::MatrixLeg {
                    matrix_key: String::new(),
                    assignment: serde_json::json!({}),
                }]
            }
        };
        let fail_fast = crate::actions::matrix::fail_fast(job.strategy.as_ref(), &ctx);
        let max_parallel = match crate::actions::matrix::max_parallel(job.strategy.as_ref(), &ctx) {
            Ok(c) => c,
            Err(e) => {
                job_err = Some(e);
                None
            }
        };
        // `runs-on` labels interpolate per leg — `matrix.*` resolves to that
        // leg's assignment, and the evaluated values drive label matching on
        // the claim path.
        let runs_on_for = |ctx: &oxidean_actions_expr::Context, notes: &mut Vec<String>| {
            job.runs_on
                .iter()
                .map(|s| crate::actions::eval_ctx::interp_or_raw(s, ctx, notes))
                .collect::<Vec<String>>()
        };
        let runs_on_eval = runs_on_for(&ctx, &mut notes);
        // `name` interpolates for display.
        let name = job
            .name
            .as_deref()
            .map(|s| crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut notes))
            .unwrap_or_else(|| job.id.clone());
        // `timeout-minutes` → absolute deadline; the evaluated seconds ride
        // the payload so a rerun can restart the clock without the YAML.
        let timeout_seconds = match &job.timeout_minutes {
            Some(src) => match crate::actions::eval_ctx::eval_minutes(src, &ctx) {
                Ok(m) => Some(m * 60.0),
                Err(e) => {
                    notes.push(format!("timeout-minutes evaluation failed: {e}"));
                    None
                }
            },
            // Absent → GitHub's 360-minute default job timeout.
            None => Some(360.0 * 60.0),
        };
        let timeout_at = timeout_seconds.map(|s| {
            (chrono::Utc::now() + chrono::Duration::milliseconds((s * 1000.0) as i64))
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
        });
        // `concurrency:` — job-level group wins over the workflow's for the
        // job's own group key. `cancel-in-progress` queues a supersede pass:
        // workflow-level groups supersede whole runs; job-level groups only
        // supersede same-group jobs in other runs.
        let concurrency_spec = job.concurrency.as_ref().or(doc.concurrency.as_ref());
        // Groups match case-insensitively on GitHub — normalize lowercase.
        let concurrency_group = concurrency_spec
            .and_then(|c| c.group.as_deref())
            .map(|s| crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut notes).to_lowercase())
            .filter(|g| !g.is_empty());
        let job_group = job
            .concurrency
            .as_ref()
            .and_then(|c| c.group.as_deref())
            .map(|s| crate::actions::eval_ctx::interp_or_raw(s, &ctx, &mut notes).to_lowercase())
            .filter(|g| !g.is_empty());
        let cancel_in_progress = concurrency_spec
            .and_then(|c| c.cancel_in_progress.as_deref())
            .and_then(|s| oxidean_actions_expr::eval_bool(s, &ctx).ok())
            .unwrap_or(false);
        if let (Some(g), true) = (&job_group, cancel_in_progress) {
            if !emit.supersede_groups.contains(g) {
                emit.supersede_groups.push(g.clone());
            }
        }
        if let Some(err) = job_err {
            job_status = "failure";
            notes.push(err);
        }
        // `environment:` — name/url interpolate at enqueue; the resolved name
        // binds protection rules (wait timer, reviewers, branch allowlist)
        // and the deployment record. Calling jobs cannot declare an
        // environment (parse rejects it), so this never fires for calls.
        let mut environment_name: Option<String> = None;
        let mut environment_url: Option<String> = None;
        let mut environment_wait_minutes: i64 = 0;
        let mut environment_reviewers = false;
        if let Some(env_spec) = &job.environment {
            let resolved_name =
                crate::actions::eval_ctx::interp_or_raw(&env_spec.name, &ctx, &mut notes);
            if resolved_name.is_empty() {
                notes.push("environment name evaluated empty — skipped".to_string());
            } else {
                environment_url = env_spec
                    .url
                    .as_deref()
                    .map(|u| crate::actions::eval_ctx::interp_or_raw(u, &ctx, &mut notes));
                environment_name = Some(resolved_name);
            }
        }
        // Protection rules attach by name; the environment row auto-creates
        // on first reference (GitHub parity). Rule failures degrade to a
        // loud job failure, never silent passthrough.
        if let Some(env_name) = &environment_name {
            match db
                .ensure_action_environment(&Uuid::new_v4().to_string(), repository_id, env_name)
                .await
            {
                Ok(env_row) => {
                    let protection: serde_json::Value =
                        serde_json::from_str(&env_row.protection_json)
                            .unwrap_or_else(|_| serde_json::json!({}));
                    environment_wait_minutes = protection
                        .get("wait_timer_minutes")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0)
                        .max(0);
                    environment_reviewers = protection
                        .get("required_reviewers")
                        .and_then(|v| v.as_array())
                        .is_some_and(|a| !a.is_empty());
                    if let Some(branches) = protection
                        .get("deployment_branches")
                        .and_then(|v| v.as_array())
                    {
                        let allowed: Vec<String> = branches
                            .iter()
                            .filter_map(|b| b.as_str().map(str::to_string))
                            .collect();
                        let short_ref = head_ref.strip_prefix("refs/heads/").unwrap_or(head_ref);
                        if !allowed.is_empty()
                            && !allowed.iter().any(|b| b == short_ref || b == head_ref)
                        {
                            job_status = "failure";
                            notes.push(format!(
                                "environment '{env_name}' does not allow deployments from ref '{head_ref}'"
                            ));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, repository_id, env_name, "environment lookup failed");
                    notes.push(format!("environment '{env_name}' lookup failed: {e}"));
                }
            }
        }
        let runs_on_shared = serde_json::to_string(&runs_on_eval).unwrap_or_else(|_| "[]".into());
        let needs_json = serde_json::to_string(&job.needs).unwrap_or_else(|_| "[]".into());
        // The full GitHub scope set rides the frozen payload — claim mints
        // `OXIDEAN_TOKEN` against it and `id-token: write` gates OIDC.
        let permission_map =
            crate::actions::effective_permissions(doc, job, emit.default_permissions);
        let permissions_json: serde_json::Map<String, serde_json::Value> = permission_map
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::Value::String(v.as_str().into())))
            .collect();
        // `secrets:` shape — `inherit` or a name→expr map evaluated at
        // expansion (raw map rides the caller payload).
        let call_secrets_json = job.secrets.as_ref().map(|s| match s {
            crate::actions::parse::CallSecretsSpec::Inherit => {
                serde_json::json!("inherit")
            }
            crate::actions::parse::CallSecretsSpec::Map(m) => m.clone(),
        });
        // Durable payload: capture everything fetch_task needs at enqueue so
        // claim never re-reads the bare repo (head could move / be GC'd).
        // `if`/`needs`/`fail_fast`/`max_parallel` ride along so the scheduler
        // can re-gate waiting jobs without the workflow YAML. `call` marks
        // caller rows for expansion; `call_inputs`/`call_secrets` mark
        // callee rows for their claim-time context overrides.
        let payload_json = serde_json::json!({
            "version": 1,
            "job_key": job.id,
            "name": name,
            "runs_on": runs_on_eval,
            "steps": job.steps,
            "env": job.env,
            "workflow_env": doc.env,
            "workflow_name": workflow_name,
            // Raw job subtree — re-emitted as the single-job workflow doc the
            // Gitea `runner.v1` surface hands stock act_runner.
            "job_yaml": job.raw_yaml,
            "outputs": job.outputs,
            "if": job.if_expr,
            "needs": job.needs,
            "fail_fast": fail_fast,
            "continue_on_error": job.continue_on_error,
            "max_parallel": max_parallel,
            "timeout_seconds": timeout_seconds,
            "container": job.container.as_ref().map(|c| crate::actions::eval_ctx::interp_json(c, &ctx, &mut notes)),
            "services": job.services.as_ref().map(|s| crate::actions::eval_ctx::interp_json(s, &ctx, &mut notes)),
            "contents_permission": crate::actions::effective_contents_permission(doc, job).as_str(),
            "permissions": serde_json::Value::Object(permissions_json),
            "environment": environment_name.clone(),
            "environment_url": environment_url.clone(),
            "call": job.uses.as_ref().map(|u| serde_json::json!({
                "uses": u,
                "with": job.with,
                "secrets": call_secrets_json,
            })),
            "call_inputs": emit.caller.as_ref().map(|c| c.inputs.clone()),
            "call_secrets": emit.caller.as_ref().map(|c| c.secrets_json.clone()),
        })
        .to_string();
        let sha = crate::auth::session::sha256_hex(payload_json.as_bytes());
        db.insert_action_payload(&sha, repository_id, workflow_path, head_sha, &payload_json)
            .await?;
        // One row per matrix leg — `max-parallel` caps how many start queued.
        // Calling legs never consume a runner slot: `waiting` until expanded.
        let mut leg_status = job_status;
        let mut queued_legs = 0usize;
        for leg in &legs {
            let job_id = Uuid::new_v4().to_string();
            if leg_status == "queued" && max_parallel.is_some_and(|c| queued_legs >= c) {
                leg_status = "waiting";
            }
            if leg_status == "queued" {
                queued_legs += 1;
            }
            // Approval-held runs: live legs insert as `waiting` + hold flag —
            // claim never sees `waiting`, so there is no claim window and the
            // CHECK-vocabulary stays intact (`waiting_approval` is derived).
            // Environment `required_reviewers` rides the same flag — a pending
            // deployment review surfaces identically (`approveJob`/`approveRun`
            // releases).
            let held = (hold_for_approval || environment_reviewers)
                && matches!(leg_status, "queued" | "waiting");
            let mut insert_status = if held { "waiting" } else { leg_status };
            // Environment wait timer — ungated legs are ready at enqueue, so
            // the deadline is stamped now and the timer sweeps promotes them.
            // Needs-gated legs get the stamp at upstream-resolve time instead.
            let deployment_not_before = if insert_status == "queued" && environment_wait_minutes > 0
            {
                insert_status = "waiting";
                Some(
                    (chrono::Utc::now() + chrono::Duration::minutes(environment_wait_minutes))
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                )
            } else {
                None
            };
            let leg_name = crate::actions::matrix::leg_display_name(&name, leg);
            let leg_ctx = if leg.assignment.as_object().is_some_and(|o| !o.is_empty()) {
                let mut lctx = ctx.clone();
                lctx.set_context("matrix", leg.assignment.clone());
                lctx
            } else {
                ctx.clone()
            };
            let leg_runs_on = if leg.assignment.as_object().is_some_and(|o| !o.is_empty()) {
                serde_json::to_string(&runs_on_for(&leg_ctx, &mut Vec::new()))
                    .unwrap_or_else(|_| "[]".into())
            } else {
                runs_on_shared.clone()
            };
            // `continue-on-error` resolves per leg — `matrix.*` interpolates.
            // Bool literals bind directly; strings evaluate as expressions;
            // eval errors degrade to `false` + a warning (an unsatisfiable
            // suppression must not silently absorb failures).
            let leg_coe = match &job.continue_on_error {
                Some(serde_json::Value::Bool(b)) => *b,
                Some(serde_json::Value::String(src)) => {
                    match oxidean_actions_expr::eval_bool(src, &leg_ctx) {
                        Ok(b) => b,
                        Err(e) => {
                            notes.push(format!(
                                "job '{}' continue-on-error '{src}' evaluation failed: {e} — treated as false",
                                job.id
                            ));
                            false
                        }
                    }
                }
                Some(serde_json::Value::Null) | None => false,
                Some(other) => {
                    notes.push(format!(
                        "job '{}' continue-on-error has non-bool value {other} — treated as false",
                        job.id
                    ));
                    false
                }
            };
            let mut insert =
                oxidean_db::JobInsert::basic(&job_id, run_id, &job.id, &leg_name, &leg_runs_on);
            insert.matrix_key = leg.matrix_key.clone();
            insert.matrix_json = leg.assignment.to_string();
            insert.needs_json = needs_json.clone();
            insert.payload_sha = Some(sha.clone());
            insert.status = insert_status.into();
            insert.approval_hold = held;
            insert.timeout_at = timeout_at.clone();
            insert.concurrency_group = concurrency_group.clone();
            insert.environment_name = environment_name.clone();
            insert.deployment_wait_minutes = environment_wait_minutes;
            insert.deployment_not_before = deployment_not_before;
            insert.outputs_json = job
                .outputs
                .as_ref()
                .map(|o| o.to_string())
                .unwrap_or_else(|| "{}".into());
            insert.caller_job_id = emit
                .caller
                .as_ref()
                .map(|c| c.caller_job_id.clone())
                .unwrap_or_default();
            insert.continue_on_error = leg_coe;
            db.insert_action_job(insert).await?;
            // The deployment record is the public projection of the
            // environment deployment — GitHub's Deployments API analogue.
            if let Some(env_name) = &environment_name {
                if let Err(e) = db
                    .insert_action_deployment(
                        &Uuid::new_v4().to_string(),
                        repository_id,
                        env_name,
                        run_id,
                        &job_id,
                        head_sha,
                        head_ref,
                        environment_url.as_deref(),
                    )
                    .await
                {
                    tracing::warn!(error = %e, run_id, env_name, "deployment insert failed");
                }
            }
            // Validation warnings + eval notes + skip/failure annotations
            // land in the job log — visible without a runner.
            let mut note = String::new();
            for w in &emit.all_warnings {
                note.push_str(&format!("::warning::{w}\n"));
            }
            for n in &notes {
                let kind = if leg_status == "failure" {
                    "error"
                } else {
                    "notice"
                };
                note.push_str(&format!("::{kind}::{n}\n"));
            }
            if held {
                let reason = if environment_reviewers {
                    format!(
                        "deployment to environment '{}' waits for reviewer approval",
                        environment_name.as_deref().unwrap_or_default()
                    )
                } else {
                    "run is waiting for maintainer approval (fork policy)".to_string()
                };
                note.push_str(&format!("::notice::{reason}\n"));
            }
            if is_call {
                note.push_str(&format!(
                    "::notice::job '{}' calls reusable workflow '{}'\n",
                    job.id,
                    job.uses.as_deref().unwrap_or_default()
                ));
            }
            if leg_status == "waiting" && has_needs {
                note.push_str(&format!(
                    "::notice::job '{}' waits on needs [{}]\n",
                    job.id,
                    job.needs.join(", ")
                ));
            }
            if !note.is_empty() {
                let dir = log_dir
                    .clone()
                    .unwrap_or_else(crate::actions::logs::resolve_log_dir);
                if let Err(e) =
                    crate::actions::logs::append_job_log(&dir, run_id, &job_id, note.as_bytes())
                        .await
                {
                    tracing::warn!(error = %e, run_id, "failed to write dispatch warnings");
                }
            }
            job_ids.push(job_id);
        }
        job_keys.push(job.id.clone());
    }
    Ok((job_ids, job_keys))
}

#[cfg(test)]
mod tests {
    use super::message_has_skip_marker;

    #[test]
    fn skip_ci_markers_all_tokens() {
        for tok in [
            "[skip ci]",
            "[ci skip]",
            "[no ci]",
            "[skip actions]",
            "[actions skip]",
        ] {
            assert!(message_has_skip_marker(&format!("subject {tok}")), "{tok}");
        }
        // Case-insensitive, anywhere in the message (subject or body).
        assert!(message_has_skip_marker("feat: x\n\nbody [SKIP CI]"));
        assert!(!message_has_skip_marker("feat: x"));
        assert!(!message_has_skip_marker("[skip c")); // partial token no
    }
}
