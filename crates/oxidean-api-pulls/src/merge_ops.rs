//! `pull.files` / `pull.commits` / `pull.merge` / `repo.mergeSettings.*`
//! handlers. Merge-gate evaluation lives in `oxidean_api_core::pull::merge_ops` (#162).

use oxidean_api_core::auth::gate::require_verified;
use oxidean_api_core::git::bare_repo_path;
use oxidean_api_core::protection::effective_for_branch;
use oxidean_api_core::pull::acl;
use oxidean_api_core::pull::merge_ops::evaluate_merge_gate;
use oxidean_api_core::pull::{db_err, load_pull_in_repo, to_public};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, MergeMethod, MergePullRequest, MergePullResponse, PullCommitSummary,
    PullCommitsResponse, PullDiffFile, PullFilesResponse, PullRefRequest, RepoGetRequest,
    RepoMergeSettings, UpdateRepoMergeSettingsRequest,
};

/// Well-known empty-tree object — the diff base for root-commit scopes.
const EMPTY_TREE_SHA: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// `pull.files` — Read+ unified diff base...head (PR-02).
pub async fn files(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullFilesResponse, AppError> {
    let req: oxidean_core::PullFilesRequest = serde_json::from_value(input)
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
        // A scoped diff may name head-side objects not reachable from the
        // pull head (fork side-branch tips); fetch them explicitly. Base-repo
        // commits that don't exist on the fork fail here harmlessly — the
        // diff below resolves them locally.
        for scoped in [
            req.sha.as_deref(),
            req.base_sha.as_deref(),
            req.head_sha.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            let _ = ctx.git.fetch_ref_from(&path, &head_path, scoped).await;
        }
    }
    // Scoped diff resolution (per-commit / range / since-last-review). The
    // default is the pull's full base...head; exactly one scope form applies.
    let sha = req.sha.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let base_sha = req
        .base_sha
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let head_sha = req
        .head_sha
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let since_review = req.since_review.unwrap_or(false);
    let (base, head): (String, String) = match (sha, base_sha, head_sha, since_review) {
        (Some(s), None, None, false) => {
            // Root commits have no parent — diff against the empty tree
            // (diff_opts peels non-commit tips via ^{tree}).
            let parent = ctx
                .git
                .cat_commit(&path, s)
                .await
                .map_err(|e| AppError::new("pull.diff_failed", format!("diff failed: {e}")))?
                .parents
                .into_iter()
                .next()
                .unwrap_or_else(|| EMPTY_TREE_SHA.to_string());
            (parent, s.to_string())
        }
        (None, Some(b), Some(h), false) => (b.to_string(), h.to_string()),
        (None, None, None, true) => {
            let Some(session) = &ctx.session else {
                return Err(AppError::new("auth.unauthenticated", "not authenticated"));
            };
            let reviewed = ctx
                .db
                .latest_review_commit_sha(&row.id, &session.user_id)
                .await
                .map_err(db_err)?
                .ok_or_else(|| {
                    AppError::new(
                        "pull.no_prior_review",
                        "no submitted review to diff against",
                    )
                })?;
            (reviewed, row.head_sha.clone())
        }
        (None, None, None, false) => (row.base_sha.clone(), row.head_sha.clone()),
        _ => {
            return Err(AppError::new(
                "rpc.bad_input",
                "scope accepts exactly one of sha, base_sha+head_sha, or since_review",
            ));
        }
    };
    let diff = ctx
        .git
        .diff_opts(
            &path,
            &base,
            &head,
            &oxidean_git::DiffOpts {
                ignore_whitespace: req.w.unwrap_or(false),
                context_lines: req.context.map(|n| n.min(20)),
            },
        )
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
    // Fork heads live in the head repo — pull the head objects into the base
    // bare repo so the base..head range resolves (same as pull.files/merge).
    oxidean_api_core::pull::ensure_head_objects_in_base(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &path,
        &row,
    )
    .await?;
    // The PR commit set is base..head — plain `git log head` would list up to
    // 100 ancestors including commits already on the base branch.
    let summaries = ctx
        .git
        .log_range(&path, &row.base_sha, &row.head_sha, 0, 100, None, None)
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
            .log_range(
                &path,
                &row.base_sha,
                &row.head_sha,
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
                &ctx.db,
                &ctx.repos_dir,
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

    // Shared executor (COL-07): the auto-merge fire path runs the same
    // post-gate merge + fan-out with the enabling user as actor.
    let sha = oxidean_api_core::pull::merge_exec::execute(
        &oxidean_api_core::merge_queue::MqDeps::from_ctx(ctx),
        &oxidean_api_core::merge_queue::QueueScope::of(&accessible),
        &row,
        oxidean_api_core::pull::merge_exec::MergeOptions {
            method: req.method,
            commit_title: req.commit_title.as_deref(),
            commit_message: req.commit_message.as_deref(),
            delete_branch: req.delete_branch.unwrap_or(false),
        },
        &oxidean_api_core::pull::merge_exec::MergeActor {
            id: &user.id,
            login: &user.username,
        },
    )
    .await?;
    let updated = load_pull_in_repo(ctx, &accessible.row.id, req.number).await?;
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
        allow_auto_merge: s.allow_auto_merge,
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
    let allow_auto = req.allow_auto_merge.unwrap_or(current.allow_auto_merge);
    ctx.db
        .set_repo_merge_settings(
            &accessible.row.id,
            allow_merge,
            allow_squash,
            allow_rebase,
            allow_auto,
        )
        .await
        .map_err(db_err)?;
    Ok(RepoMergeSettings {
        allow_merge_commit: allow_merge,
        allow_squash_merge: allow_squash,
        allow_rebase_merge: allow_rebase,
        allow_auto_merge: allow_auto,
    })
}
