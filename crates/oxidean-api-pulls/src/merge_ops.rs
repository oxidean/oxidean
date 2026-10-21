//! `pull.files` / `pull.commits` / `pull.merge` / `repo.mergeSettings.*`
//! handlers. Merge-gate evaluation lives in `oxidean_api_core::pull::merge_ops` (#162).

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::notify;
use oxidean_api_core::protection::effective_for_branch;
use oxidean_api_core::pull::acl;
use oxidean_api_core::pull::merge_ops::evaluate_merge_gate;
use oxidean_api_core::pull::{db_err, load_pull_in_repo, to_public};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_api_core::webhook::dispatch;
use oxidean_core::{
    AppError, MergeMethod, MergePullRequest, MergePullResponse, PullCommitSummary,
    PullCommitsResponse, PullDiffFile, PullFilesResponse, PullRefRequest, RepoGetRequest,
    RepoMergeSettings, UpdateRepoMergeSettingsRequest,
};

fn parse_closing_issue_numbers(text: &str) -> Vec<i64> {
    let lower = text.to_lowercase();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lower.len() {
        let rest = &lower[i..];
        let hit = rest
            .find("fixes #")
            .or_else(|| rest.find("closes #"))
            .or_else(|| rest.find("resolves #"));
        let Some(pos) = hit else { break };
        let hash_rel = rest[pos..].find('#').unwrap();
        let after_hash = i + pos + hash_rel + 1;
        let digits: String = lower[after_hash..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        if let Ok(n) = digits.parse::<i64>() {
            if n > 0 && !out.contains(&n) {
                out.push(n);
            }
        }
        i = after_hash + digits.len().max(1);
    }
    out
}

/// `pull.files` — Read+ unified diff base...head (PR-02).
pub async fn files(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullFilesResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.files input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    if row.head_repo_id != row.repo_id {
        let head_repo = ctx
            .db
            .find_repository_by_id(&row.head_repo_id)
            .await
            .map_err(db_err)?
            .ok_or_else(|| AppError::new("pull.internal", "head repo missing"))?;
        let head_owner = match head_repo.owner_type.as_str() {
            "org" => ctx
                .db
                .find_organization_by_id(&head_repo.owner_id)
                .await
                .map_err(db_err)?
                .map(|o| o.slug)
                .unwrap_or_default(),
            _ => ctx
                .db
                .find_user_by_id(&head_repo.owner_id)
                .await
                .map_err(db_err)?
                .map(|u| u.username)
                .unwrap_or_default(),
        };
        let head_path = bare_repo_path(&ctx.repos_dir, &head_owner, &head_repo.name)?;
        let _ = ctx
            .git
            .fetch_ref_from(&path, &head_path, &row.head_sha)
            .await;
    }
    let diff = ctx
        .git
        .diff(&path, &row.base_sha, &row.head_sha)
        .await
        .map_err(|e| AppError::new("pull.diff_failed", format!("diff failed: {e}")))?;
    Ok(PullFilesResponse {
        empty: diff.empty,
        files: diff
            .files
            .into_iter()
            .map(|f| PullDiffFile {
                path: f.path,
                status: f.status,
                patch: f.patch,
            })
            .collect(),
    })
}

/// `pull.commits` — Read+.
pub async fn commits(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<PullCommitsResponse, AppError> {
    let req: PullRefRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.commits input: {e}")))?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let summaries = ctx
        .git
        .log(&path, &row.head_ref, 0, 100, None, None)
        .await
        .map_err(|e| AppError::new("pull.commits_failed", format!("log failed: {e}")))?;
    let mut emails: Vec<String> = Vec::new();
    for c in &summaries {
        if !c.committer_email.trim().is_empty() {
            emails.push(c.committer_email.clone());
        }
        emails.push(c.author_email.clone());
    }
    // Resolve all committer/author emails once — the same map drives the
    // response rows, the signature keyring, and the verified policy.
    let resolved = oxidean_api_core::repo::author_resolve::resolve_authors_for_emails(
        &ctx.db,
        emails.iter().map(String::as_str),
    )
    .await;
    let keyring =
        oxidean_api_core::repo::signatures::keyring_for_resolved(&ctx.db, &emails, &resolved).await;
    let summaries = if keyring.has_any() {
        ctx.git
            .log(
                &path,
                &row.head_ref,
                0,
                100,
                keyring.allowed_signers.as_deref(),
                keyring.gpg_home.as_deref(),
            )
            .await
            .map_err(|e| AppError::new("pull.commits_failed", format!("log failed: {e}")))?
    } else {
        summaries
    };
    let (any_valid, any_gpg) = oxidean_api_core::repo::signatures::needs_verified_policy(
        summaries
            .iter()
            .map(|c| (&c.signature_status, &c.signature_kind)),
    );
    let policy = if any_valid {
        let uids: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            resolved
                .values()
                .filter_map(|r| r.user_id.clone())
                .filter(|id| seen.insert(id.clone()))
                .collect()
        };
        oxidean_api_core::repo::signatures::VerifiedPolicy::load(&ctx.db, &uids, any_gpg).await
    } else {
        oxidean_api_core::repo::signatures::VerifiedPolicy::none()
    };
    let mut commits = Vec::with_capacity(summaries.len());
    for c in summaries {
        let r = resolved.get(&c.author_email).cloned().unwrap_or_default();
        let signature_status = policy.status(
            &resolved,
            &c.committer_email,
            &c.author_email,
            &c.signature_status,
            &c.signature_kind,
        );
        commits.push(PullCommitSummary {
            sha: c.sha,
            short_sha: c.short_sha,
            subject: c.subject,
            author_name: c.author_name,
            author_email: c.author_email,
            authored_at: c.authored_at,
            author_user_id: r.user_id,
            author_username: r.username,
            author_avatar_url: r.avatar_url,
            signature_status,
            signature_kind: c.signature_kind,
        });
    }
    Ok(PullCommitsResponse { commits })
}

/// `pull.merge` — Write+ (PR-05 / D-PR-17..23).
pub async fn merge(ctx: &RpcCtx, input: serde_json::Value) -> Result<MergePullResponse, AppError> {
    let user = require_verified(ctx).await?;
    let req: MergePullRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.merge input: {e}")))?;
    let accessible = acl::resolve_for_write(ctx, &req.owner, &req.name).await?;
    let row = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    if row.state != "open" {
        return Err(AppError::new(
            "pull.invalid_state",
            "only open pull requests can be merged",
        ));
    }
    let settings = ctx
        .db
        .get_repo_merge_settings(&accessible.row.id)
        .await
        .map_err(db_err)?;
    match req.method {
        MergeMethod::Merge if !settings.allow_merge_commit => {
            return Err(AppError::new(
                "pull.merge_disabled",
                "merge commits are disabled for this repository",
            ));
        }
        MergeMethod::Squash if !settings.allow_squash_merge => {
            return Err(AppError::new(
                "pull.merge_disabled",
                "squash merges are disabled for this repository",
            ));
        }
        MergeMethod::Rebase if !settings.allow_rebase_merge => {
            return Err(AppError::new(
                "pull.merge_disabled",
                "rebase merges are disabled for this repository",
            ));
        }
        _ => {}
    }

    // Phase 13 / PR-08: shared protection evaluate against base branch (D-22).
    {
        let eff = effective_for_branch(&ctx.db, &accessible.row.id, &row.base_ref).await?;
        if eff.matched {
            // GitHub's "Require merge queue" ruleset — direct merges are
            // never permitted; `merge_queue.enqueue` routes the PR through
            // group verification instead.
            if eff.required_merge_queue {
                return Err(AppError::new(
                    "pull.merge_queue_required",
                    "merge queue is required for the base branch — add this pull request to the merge queue instead",
                ));
            }
            evaluate_merge_gate(
                ctx,
                &accessible,
                &eff,
                &row,
                Some(match req.method {
                    MergeMethod::Merge => "merge".into(),
                    MergeMethod::Squash => "squash".into(),
                    MergeMethod::Rebase => "rebase".into(),
                }),
            )
            .await?;
        }
    }

    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    oxidean_api_core::pull::ensure_head_objects_in_base(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &path,
        &row,
    )
    .await?;

    let message = req
        .commit_title
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(row.title.as_str());
    let full_message = match req
        .commit_message
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(body) => format!("{message}\n\n{body}"),
        None => message.to_string(),
    };

    let sha = match req.method {
        MergeMethod::Merge => {
            ctx.git
                .merge_commit(&path, &row.base_ref, &row.head_sha, &full_message)
                .await
        }
        MergeMethod::Squash => {
            ctx.git
                .squash_merge(&path, &row.base_ref, &row.head_sha, &full_message)
                .await
        }
        MergeMethod::Rebase => {
            ctx.git
                .rebase_merge(&path, &row.base_ref, &row.head_sha)
                .await
        }
    }
    .map_err(|e| {
        let msg = e.to_string().to_lowercase();
        if msg.contains("conflict") {
            AppError::new("pull.merge_conflict", "merge conflict")
        } else {
            AppError::new("pull.merge_failed", format!("merge failed: {e}"))
        }
    })?;

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    ctx.db
        .mark_pull_merged(&row.id, &user.id, &sha, req.method.as_str(), &now)
        .await
        .map_err(db_err)?;

    // API-06: refs/pull/{N}/merge resolves to the recorded merge commit — the
    // merge object was pushed into the base repo by the worktree merge above.
    // (`/head` keeps tracking pull.head_sha; while the PR was open /merge held
    // the actions test-merge commit — the real merge repoints it here.)
    oxidean_api_core::pull::refs::sync_merge_ref(
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
        row.number,
        &sha,
    )
    .await;

    let merge_subject = full_message.lines().next().unwrap_or("").trim();
    oxidean_api_core::repo::record_pr_merge(
        &ctx.db,
        &accessible.row.id,
        &user.id,
        &row.base_ref,
        &sha,
        row.number,
        if merge_subject.is_empty() {
            None
        } else {
            Some(merge_subject)
        },
    )
    .await;

    oxidean_api_core::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        accessible.row.id.clone(),
    );

    // GIT-25: refresh cached size_bytes after merge objects landed.
    if let Err(e) =
        oxidean_api_core::git::quota::refresh_repo_size_bytes(&ctx.db, &accessible.row.id, &path)
            .await
    {
        tracing::warn!(error = %e, "refresh repo size_bytes failed");
    }

    if row.base_ref == accessible.row.default_branch {
        let mut nums = parse_closing_issue_numbers(&row.body);
        nums.extend(parse_closing_issue_numbers(&full_message));
        nums.sort_unstable();
        nums.dedup();
        for n in nums {
            if let Ok(Some(issue)) = ctx
                .db
                .find_issue_by_repo_number(&accessible.row.id, n)
                .await
            {
                if issue.state == "open" {
                    let _ = ctx.db.close_issue(&issue.id, &user.id).await;
                }
            }
        }
    }

    if req.delete_branch.unwrap_or(false)
        && row.head_repo_id == row.repo_id
        && ctx.git.branch_delete(&path, &row.head_ref).await.is_ok()
    {
        // API-04: deleting the head branch on merge fires `delete`.
        let payload = dispatch::ref_event_payload(
            "delete",
            &row.head_ref,
            "branch",
            &accessible.row.default_branch,
            &accessible.row.description,
            &accessible.owner_username,
            &accessible.row.name,
            &accessible.row.id,
            &user.username,
            &user.id,
        );
        dispatch::emit(
            &ctx.db,
            &accessible.row.id,
            "delete",
            "",
            payload,
            &ctx.env_name,
        )
        .await;
    }

    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    oxidean_api_core::pull::emit_pull_event(
        ctx,
        &accessible,
        &updated,
        "closed",
        &user.username,
        &user.id,
        true,
    )
    .await;
    // `on: pull_request types: [closed]` — merged reads true in the
    // payload (GitHub fires this on merge, same as the webhook).
    let (head_owner, head_name) = if updated.head_repo_id == updated.repo_id {
        (
            accessible.owner_username.clone(),
            accessible.row.name.clone(),
        )
    } else if let Ok(Some(head_repo)) = ctx.db.find_repository_by_id(&updated.head_repo_id).await {
        let owner = match head_repo.owner_type.as_str() {
            "org" => ctx
                .db
                .find_organization_by_id(&head_repo.owner_id)
                .await
                .ok()
                .flatten()
                .map(|o| o.slug)
                .unwrap_or_default(),
            _ => ctx
                .db
                .find_user_by_id(&head_repo.owner_id)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
                .unwrap_or_default(),
        };
        (owner, head_repo.name)
    } else {
        (
            accessible.owner_username.clone(),
            accessible.row.name.clone(),
        )
    };
    oxidean_api_core::pull::notify_actions_for_pull(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &updated,
        &accessible.owner_username,
        &accessible.row.name,
        &head_owner,
        &head_name,
        Some(&user.id),
        oxidean_api_core::actions::events::rpc_caller_is_job_token(ctx),
        oxidean_api_core::actions::PullRequestAction::Closed,
        true,
    )
    .await;
    let subject = notify::subject_for_pull(&updated);
    let recipients = notify::pull_participant_ids(&ctx.db, &updated.id, &updated.author_id).await;
    notify::fanout_activity(&ctx.db, &user.id, recipients, "pr_merged", &subject).await;
    let pull = to_public(ctx, &updated).await?;
    Ok(MergePullResponse {
        pull,
        merge_commit_sha: sha,
    })
}

/// `repo.mergeSettings.get` — repo Read+; NOT gated on the pulls unit so the
/// merge-settings panel in repository settings stays usable while pulls are
/// disabled (COL-13).
pub async fn merge_settings_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMergeSettings, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid merge settings get: {e}")))?;
    let accessible =
        oxidean_api_core::repo::resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let s = ctx
        .db
        .get_repo_merge_settings(&accessible.row.id)
        .await
        .map_err(db_err)?;
    Ok(RepoMergeSettings {
        allow_merge_commit: s.allow_merge_commit,
        allow_squash_merge: s.allow_squash_merge,
        allow_rebase_merge: s.allow_rebase_merge,
    })
}

pub async fn merge_settings_update(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMergeSettings, AppError> {
    let _user = require_verified(ctx).await?;
    let req: UpdateRepoMergeSettingsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid merge settings update: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_admin(ctx, &req.owner, &req.name).await?;
    let current = ctx
        .db
        .get_repo_merge_settings(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let allow_merge = req.allow_merge_commit.unwrap_or(current.allow_merge_commit);
    let allow_squash = req.allow_squash_merge.unwrap_or(current.allow_squash_merge);
    let allow_rebase = req.allow_rebase_merge.unwrap_or(current.allow_rebase_merge);
    ctx.db
        .set_repo_merge_settings(&accessible.row.id, allow_merge, allow_squash, allow_rebase)
        .await
        .map_err(db_err)?;
    Ok(RepoMergeSettings {
        allow_merge_commit: allow_merge,
        allow_squash_merge: allow_squash,
        allow_rebase_merge: allow_rebase,
    })
}
