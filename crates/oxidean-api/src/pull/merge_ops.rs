//! Merge, files, commits, and merge-settings handlers.

use oxidean_core::{
    AppError, MergeMethod, MergePullRequest, MergePullResponse, PullCommitSummary,
    PullCommitsResponse, PullDiffFile, PullFilesResponse, PullRefRequest, RepoGetRequest,
    RepoMergeSettings, UpdateRepoMergeSettingsRequest,
};
use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::notify;
use crate::protection::{
    effective_for_branch, evaluate_merge, MergeEvalInput,
};
use crate::pull::acl;
use crate::rpc::RpcCtx;

use super::{db_err, load_pull_in_repo, to_public};

async fn git_is_ancestor(bare: &std::path::Path, maybe_ancestor: &str, tip: &str) -> Result<bool, String> {
    if maybe_ancestor.is_empty() || tip.is_empty() || maybe_ancestor == tip {
        return Ok(true);
    }
    let bare_str = bare.to_str().ok_or("non-utf8 bare path")?;
    let out = tokio::process::Command::new("git")
        .args([
            "-C",
            bare_str,
            "merge-base",
            "--is-ancestor",
            maybe_ancestor,
            tip,
        ])
        .output()
        .await
        .map_err(|e| format!("git merge-base: {e}"))?;
    Ok(out.status.success())
}

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
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid pull.files input: {e}"))
    })?;
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
    let req: PullRefRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid pull.commits input: {e}"),
        )
    })?;
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
    let resolved = crate::repo::author_resolve::resolve_authors_for_emails(
        &ctx.db,
        emails.iter().map(String::as_str),
    )
    .await;
    let keyring = crate::repo::signatures::keyring_for_resolved(&ctx.db, &emails, &resolved).await;
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
    let (any_valid, any_gpg) = crate::repo::signatures::needs_verified_policy(
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
        crate::repo::signatures::VerifiedPolicy::load(&ctx.db, &uids, any_gpg).await
    } else {
        crate::repo::signatures::VerifiedPolicy::none()
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
    let req: MergePullRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new("rpc.bad_input", format!("invalid pull.merge input: {e}"))
    })?;
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
            let reviews = ctx
                .db
                .list_pull_reviews(&row.id)
                .await
                .map_err(db_err)?;
            // Latest-per-user review state (D-PR-07); dismiss_stale excludes Approves on older SHAs.
            let mut latest: std::collections::BTreeMap<String, &oxidean_db::PullReviewRow> =
                std::collections::BTreeMap::new();
            for r in &reviews {
                match latest.get(&r.author_id) {
                    Some(prev) if prev.submitted_at >= r.submitted_at => {}
                    _ => {
                        latest.insert(r.author_id.clone(), r);
                    }
                }
            }
            let mut approving_reviewer_ids = Vec::new();
            for r in latest.values() {
                if r.state != "approved" {
                    continue;
                }
                if eff.dismiss_stale_reviews {
                    if r.commit_sha.as_deref() != Some(row.head_sha.as_str()) {
                        continue;
                    }
                }
                approving_reviewer_ids.push(r.author_id.clone());
            }
            let comments = ctx
                .db
                .list_pull_comments(&row.id)
                .await
                .map_err(db_err)?;
            let unresolved = comments
                .iter()
                .filter(|c| c.path.is_some() && !c.resolved)
                .count() as i32;
            let statuses = ctx
                .db
                .list_commit_statuses(&accessible.row.id, &row.head_sha)
                .await
                .map_err(db_err)?;
            let mut status_by_context = std::collections::BTreeMap::new();
            for s in statuses {
                status_by_context.insert(s.context, s.state);
            }
            let path_for_anc = bare_repo_path(
                &ctx.repos_dir,
                &accessible.owner_username,
                &accessible.row.name,
            )?;
            let head_up_to_date = git_is_ancestor(&path_for_anc, &row.base_sha, &row.head_sha)
                .await
                .unwrap_or(true);
            let merge_input = MergeEvalInput {
                approving_review_count: approving_reviewer_ids.len() as i32,
                unresolved_review_threads: unresolved,
                last_head_pusher_id: None,
                approving_reviewer_ids,
                head_sha: row.head_sha.clone(),
                base_sha: row.base_sha.clone(),
                head_up_to_date,
                merge_method: Some(match req.method {
                    MergeMethod::Merge => "merge".into(),
                    MergeMethod::Squash => "squash".into(),
                    MergeMethod::Rebase => "rebase".into(),
                }),
                is_draft: row.draft,
                status_by_context,
            };
            evaluate_merge(&eff, accessible.capability, &merge_input)?;
        }
    }

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
        ctx.git
            .fetch_ref_from(&path, &head_path, &row.head_sha)
            .await
            .map_err(|e| AppError::new("pull.merge_failed", format!("fetch head failed: {e}")))?;
    }

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

    let merge_subject = full_message.lines().next().unwrap_or("").trim();
    crate::repo::record_pr_merge(
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

    crate::mirror::notify_mirror_after_local_mutation(
        ctx.db.clone(),
        ctx.git.clone(),
        ctx.repos_dir.clone(),
        accessible.row.id.clone(),
    );

    if row.base_ref == accessible.row.default_branch {
        let mut nums = parse_closing_issue_numbers(&row.body);
        nums.extend(parse_closing_issue_numbers(&full_message));
        nums.sort_unstable();
        nums.dedup();
        for n in nums {
            if let Ok(Some(issue)) = ctx.db.find_issue_by_repo_number(&accessible.row.id, n).await {
                if issue.state == "open" {
                    let _ = ctx.db.close_issue(&issue.id, &user.id).await;
                }
            }
        }
    }

    if req.delete_branch.unwrap_or(false) && row.head_repo_id == row.repo_id {
        let _ = ctx.git.branch_delete(&path, &row.head_ref).await;
    }

    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
    super::emit_pull_event(
        ctx,
        &accessible,
        &updated,
        "closed",
        &user.username,
        &user.id,
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

pub async fn merge_settings_get(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoMergeSettings, AppError> {
    let req: RepoGetRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid merge settings get: {e}"),
        )
    })?;
    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
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
        .set_repo_merge_settings(
            &accessible.row.id,
            allow_merge,
            allow_squash,
            allow_rebase,
        )
        .await
        .map_err(db_err)?;
    Ok(RepoMergeSettings {
        allow_merge_commit: allow_merge,
        allow_squash_merge: allow_squash,
        allow_rebase_merge: allow_rebase,
    })
}
