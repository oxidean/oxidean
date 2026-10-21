//! Pull-request domain shared layer — ACL, refs, merge-gate evaluation,
//! synchronize/event fan-out, and the `pull.create` shared entry
//! (`repo::files` create-PR flow + webhook/merge_queue callers). The
//! `pull.*` RPC handlers live in `oxidean-api-pulls` (issue #162).

pub mod acl;
pub mod merge_ops;
pub mod refs;

use std::path::Path;

use oxidean_core::{
    AppError, CreatePullRequest, IssueAssigneePublic, MergeMethod, PullPublic, PullState,
};
use oxidean_db::{Database, PullRow};
use oxidean_git::{CliGitBackend, GitBackend};
use uuid::Uuid;

use crate::auth::gate::require_verified;
use crate::git::bare_repo_path;
use crate::notify;
use crate::protection;
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;
use crate::webhook::payloads;

const TITLE_MAX_CHARS: usize = 1_024;
const BODY_MAX_CHARS: usize = 65_536;

pub fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!("pull db error: {e}");
        AppError::new("pull.internal", "pull operation failed")
    }
}

#[allow(clippy::too_many_arguments)]
/// Soft-fail PR → Actions enqueue (CR-02 / D-ACT-05). Never fails the PR mutation.
pub async fn notify_actions_for_pull(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    pull: &PullRow,
    base_owner: &str,
    base_name: &str,
    head_owner: &str,
    head_name: &str,
    actor_id: Option<&str>,
    job_token_actor: bool,
    action: crate::actions::PullRequestAction,
    // `pull_request.merged` on `closed` events.
    merged: bool,
) {
    let Ok(bare) = bare_repo_path(repos_dir, head_owner, head_name) else {
        return;
    };
    let Ok(base_bare) = bare_repo_path(repos_dir, base_owner, base_name) else {
        return;
    };
    let base_default_branch = db
        .find_repository_by_id(&pull.repo_id)
        .await
        .ok()
        .flatten()
        .map(|r| r.default_branch)
        .unwrap_or_else(|| "main".into());
    let event = crate::actions::PullRequestEvent {
        action,
        repository_id: pull.repo_id.to_string(),
        bare_repo: bare,
        base_bare,
        head_sha: pull.head_sha.clone(),
        head_ref: pull.head_ref.clone(),
        base_ref: pull.base_ref.clone(),
        base_sha: pull.base_sha.clone(),
        pull_number: pull.number,
        pull_title: pull.title.clone(),
        pull_author_id: Some(pull.author_id.clone()),
        head_owner: head_owner.to_string(),
        head_name: head_name.to_string(),
        base_owner: base_owner.to_string(),
        base_name: base_name.to_string(),
        base_default_branch,
        actor_id: actor_id.map(str::to_string),
        head_repo_id: Some(pull.head_repo_id.clone()),
        merged,
        job_token_actor,
        actions_enabled_instance: crate::actions::env_actions_enabled(),
    };
    crate::actions::notify_pull_request_actions(db, git, &event).await;
}

pub async fn emit_pull_event(
    ctx: &RpcCtx,
    accessible: &crate::repo::AccessibleRepo,
    row: &PullRow,
    action: &str,
    sender_login: &str,
    sender_id: &str,
    merged: bool,
) {
    emit_pull_event_db(
        &ctx.db,
        &accessible.owner_username,
        &accessible.row.name,
        &accessible.row.id,
        row,
        action,
        sender_login,
        sender_id,
        merged,
        &ctx.env_name,
        None,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
/// Emit a `pull_request` webhook from git push paths (no [`RpcCtx`]).
pub async fn emit_pull_event_db(
    db: &Database,
    base_owner: &str,
    base_name: &str,
    base_repo_id: &str,
    row: &PullRow,
    action: &str,
    sender_login: &str,
    sender_id: &str,
    merged: bool,
    env_name: &str,
    dequeue_reason: Option<&str>,
) {
    let (head_owner, head_name) = if row.head_repo_id == row.repo_id {
        (base_owner.to_string(), base_name.to_string())
    } else if let Ok(Some(head_repo)) = db.find_repository_by_id(&row.head_repo_id).await {
        let owner = match head_repo.owner_type.as_str() {
            "org" => db
                .find_organization_by_id(&head_repo.owner_id)
                .await
                .ok()
                .flatten()
                .map(|o| o.slug)
                .unwrap_or_default(),
            _ => db
                .find_user_by_id(&head_repo.owner_id)
                .await
                .ok()
                .flatten()
                .map(|u| u.username)
                .unwrap_or_default(),
        };
        (owner, head_repo.name)
    } else {
        (base_owner.to_string(), base_name.to_string())
    };
    let state = if merged { "closed" } else { row.state.as_str() };
    let mut payload = payloads::pull_request_payload(
        action,
        row.number,
        &row.title,
        &row.body,
        state,
        row.draft,
        merged,
        &row.base_ref,
        &row.head_ref,
        &row.head_sha,
        &head_owner,
        &head_name,
        base_owner,
        base_name,
        base_repo_id,
        sender_login,
        sender_id,
    );
    if let Some(reason) = dequeue_reason {
        payload["reason"] = serde_json::Value::String(reason.to_string());
    }
    dispatch::emit(db, base_repo_id, "pull_request", action, payload, env_name).await;
}

/// Pull the PR head commit into the base bare when the head lives in a
/// fork — `merge_commit`/`test_merge_commit` on the base object store need
/// the objects present. Same-repo heads are already local (no-op).
pub async fn ensure_head_objects_in_base(
    db: &Database,
    git: &dyn GitBackend,
    repos_dir: &Path,
    base_bare: &Path,
    pull: &PullRow,
) -> Result<(), AppError> {
    if pull.head_repo_id == pull.repo_id {
        return Ok(());
    }
    let head_repo = db
        .find_repository_by_id(&pull.head_repo_id)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("pull.internal", "head repo missing"))?;
    let head_owner = match head_repo.owner_type.as_str() {
        "org" => db
            .find_organization_by_id(&head_repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|o| o.slug)
            .unwrap_or_default(),
        _ => db
            .find_user_by_id(&head_repo.owner_id)
            .await
            .map_err(db_err)?
            .map(|u| u.username)
            .unwrap_or_default(),
    };
    let head_path = bare_repo_path(repos_dir, &head_owner, &head_repo.name)?;
    git.fetch_ref_from(base_bare, &head_path, &pull.head_sha)
        .await
        .map_err(|e| AppError::new("pull.merge_failed", format!("fetch head failed: {e}")))?;
    Ok(())
}

pub async fn resolve_ref_sha_at(
    git: &dyn GitBackend,
    repos_dir: &Path,
    owner: &str,
    repo_name: &str,
    refname: &str,
) -> Option<String> {
    let path = bare_repo_path(repos_dir, owner, repo_name).ok()?;
    let refs = git.list_refs(&path).await.ok()?;
    let want = if refname.starts_with("refs/") {
        refname.to_string()
    } else {
        format!("refs/heads/{refname}")
    };
    refs.iter()
        .find(|r| r.name == want || r.name.ends_with(&format!("/{refname}")) || r.oid == refname)
        .map(|r| r.oid.clone())
}

#[allow(clippy::too_many_arguments)]
/// After a successful push: update open same-repo PR heads, dismiss stale approvals, emit synchronize.
pub async fn synchronize_after_push(
    db: &Database,
    repos_dir: &Path,
    repository_id: &str,
    owner: &str,
    repo_name: &str,
    pusher_login: &str,
    pusher_id: &str,
    updates: &[(String, String, String)],
    env_name: &str,
    // GITHUB_TOKEN parity — job-token pushes must not fire `pull_request`
    // runs off the synchronize.
    job_token_actor: bool,
) {
    let git = CliGitBackend::new();
    let mut branch_after: Vec<(String, String)> = updates
        .iter()
        .filter_map(|(_before, after, refname)| {
            let branch = protection::branch_from_ref(refname)?;
            if after.chars().all(|c| c == '0') {
                return None;
            }
            Some((branch.to_string(), after.clone()))
        })
        .collect();

    // SSH notify_push often has no pkt-line updates — refresh open same-repo heads from live refs.
    if branch_after.is_empty() {
        let (open, _) = match db
            .list_pulls_for_repo(repository_id, Some("open"), 0, 500)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "synchronize_after_push: list open pulls failed");
                return;
            }
        };
        for pull in open.into_iter().filter(|p| p.head_repo_id == repository_id) {
            if let Some(sha) =
                resolve_ref_sha_at(&git, repos_dir, owner, repo_name, &pull.head_ref).await
            {
                if sha != pull.head_sha {
                    branch_after.push((pull.head_ref.clone(), sha));
                }
            }
        }
        // Dedupe by branch name (last wins).
        let mut seen = std::collections::BTreeMap::<String, String>::new();
        for (b, s) in branch_after.drain(..) {
            seen.insert(b, s);
        }
        branch_after = seen.into_iter().collect();
    }

    for (branch, after_sha) in branch_after {
        if let Err(e) = sync_open_pulls_for_branch(
            db,
            repos_dir,
            &git,
            repository_id,
            owner,
            repo_name,
            &branch,
            &after_sha,
            pusher_login,
            pusher_id,
            env_name,
            job_token_actor,
        )
        .await
        {
            tracing::warn!(
                error = %e,
                %branch,
                "synchronize_after_push: branch sync failed (soft-fail)"
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn sync_open_pulls_for_branch(
    db: &Database,
    repos_dir: &Path,
    git: &dyn GitBackend,
    repository_id: &str,
    owner: &str,
    repo_name: &str,
    branch: &str,
    after_sha: &str,
    pusher_login: &str,
    pusher_id: &str,
    env_name: &str,
    job_token_actor: bool,
) -> Result<(), String> {
    let (open, _) = db
        .list_pulls_for_repo(repository_id, Some("open"), 0, 500)
        .await?;
    let matching: Vec<PullRow> = open
        .into_iter()
        .filter(|p| p.head_repo_id == repository_id && p.head_ref == branch)
        .collect();
    if matching.is_empty() {
        return Ok(());
    }

    for pull in matching {
        synchronize_pull_after_head_move(
            db,
            repos_dir,
            git,
            &pull,
            owner,
            repo_name,
            owner,
            repo_name,
            after_sha,
            pusher_login,
            pusher_id,
            env_name,
            job_token_actor,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
/// After a pull's head moved to `new_head_sha` — either via a same-repo push
/// (`sync_open_pulls_for_branch`) or via `pull.updateBranch` / `repo.syncFork`
/// on the head repo — run the synchronize bookkeeping: refresh stored base SHA,
/// mark line comments outdated, dismiss stale approvals per base-branch
/// protection, emit `pull_request:synchronize`, and soft-notify Actions.
///
/// `base_owner`/`base_name` identify the repo hosting the PR (`pull.repo_id`);
/// `head_owner`/`head_name` identify the bare holding the new head tip (same for
/// same-repo PRs, the fork for cross-repo PRs). Returns the refreshed row.
pub async fn synchronize_pull_after_head_move(
    db: &Database,
    repos_dir: &Path,
    git: &dyn GitBackend,
    pull: &PullRow,
    base_owner: &str,
    base_name: &str,
    head_owner: &str,
    head_name: &str,
    new_head_sha: &str,
    actor_login: &str,
    actor_id: &str,
    env_name: &str,
    // GITHUB_TOKEN parity — token-caused head moves never start runs.
    job_token_actor: bool,
) -> Result<PullRow, String> {
    if pull.head_sha == new_head_sha {
        return Ok(pull.clone());
    }
    let base_sha = resolve_ref_sha_at(git, repos_dir, base_owner, base_name, &pull.base_ref)
        .await
        .unwrap_or_else(|| pull.base_sha.clone());
    if base_sha != pull.base_sha {
        db.update_pull_fields(
            &pull.id,
            &pull.title,
            &pull.body,
            pull.draft,
            &pull.base_ref,
            &base_sha,
        )
        .await?;
    }
    db.update_pull_head_sha(&pull.id, new_head_sha).await?;
    // API-06: keep refs/pull/{N}/head tracking the new head tip. The ref lives
    // in the PR's *base* repo — for fork heads `sync_head_ref` fetches the
    // object in first (the head tip may belong to a fork bare).
    let mut pull_for_ref = pull.clone();
    pull_for_ref.head_sha = new_head_sha.to_string();
    refs::sync_head_ref(git, repos_dir, db, base_owner, base_name, &pull_for_ref).await;
    let _ = db.mark_pull_line_comments_outdated(&pull.id).await;

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let eff = protection::effective_for_branch(db, &pull.repo_id, &pull.base_ref)
        .await
        .map_err(|e| e.message)?;
    if eff.dismiss_stale_reviews {
        if let Ok(reviews) = db.list_pull_reviews(&pull.id).await {
            for r in reviews {
                if r.state == "approved" && r.commit_sha.as_deref() != Some(new_head_sha) {
                    let _ = db
                        .dismiss_pull_review(
                            &r.id,
                            Some("New commits pushed to the head branch"),
                            &now,
                        )
                        .await;
                }
            }
        }
    }

    let updated = db
        .find_pull_by_repo_number(&pull.repo_id, pull.number)
        .await?
        .ok_or_else(|| "pull missing after synchronize".to_string())?;
    emit_pull_event_db(
        db,
        base_owner,
        base_name,
        &pull.repo_id,
        &updated,
        "synchronize",
        actor_login,
        actor_id,
        false,
        env_name,
        None,
    )
    .await;
    notify_actions_for_pull(
        db,
        git,
        repos_dir,
        &updated,
        base_owner,
        base_name,
        head_owner,
        head_name,
        Some(actor_id),
        job_token_actor,
        crate::actions::PullRequestAction::Synchronize,
        false,
    )
    .await;
    // Merge queue: a queued PR whose head moved leaves the train — GitHub
    // removes it (the queued commit is stale).
    crate::merge_queue::after_pull_head_moved(
        &crate::merge_queue::MqDeps {
            db: db.clone(),
            git: std::sync::Arc::new(oxidean_git::CliGitBackend::new()),
            repos_dir: repos_dir.to_path_buf(),
            pages_dir: crate::pages::resolve_pages_dir(),
            env_name: env_name.to_string(),
            job_token_actor,
            actions_enabled: crate::actions::env_actions_enabled(),
        },
        &pull.repo_id,
        &updated,
        actor_login,
        actor_id,
    )
    .await;
    Ok(updated)
}

pub fn pull_not_found() -> AppError {
    AppError::new("pull.not_found", "Pull request not found")
}

pub fn validate_title(title: &str) -> Result<&str, AppError> {
    let t = title.trim();
    if t.is_empty() {
        return Err(AppError::new("rpc.bad_input", "title is required"));
    }
    if t.chars().count() > TITLE_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("title exceeds {TITLE_MAX_CHARS} characters"),
        ));
    }
    Ok(t)
}

pub fn validate_body(body: Option<&str>) -> Result<String, AppError> {
    let stored = body.unwrap_or("").to_string();
    if stored.chars().count() > BODY_MAX_CHARS {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("body exceeds {BODY_MAX_CHARS} characters"),
        ));
    }
    Ok(stored)
}

pub fn validate_ref_name(name: &str, field: &str) -> Result<String, AppError> {
    let t = name.trim();
    if t.is_empty() {
        return Err(AppError::new(
            "rpc.bad_input",
            format!("{field} is required"),
        ));
    }
    if t.contains('\0') || t.contains("..") {
        return Err(AppError::new("rpc.bad_input", format!("invalid {field}")));
    }
    Ok(t.to_string())
}

pub async fn resolve_ref_sha(
    ctx: &RpcCtx,
    owner: &str,
    repo_name: &str,
    refname: &str,
) -> Result<String, AppError> {
    let path = bare_repo_path(&ctx.repos_dir, owner, repo_name)?;
    let refs =
        ctx.git.list_refs(&path).await.map_err(|e| {
            AppError::new("pull.ref_not_found", format!("could not list refs: {e}"))
        })?;
    let want = if refname.starts_with("refs/") {
        refname.to_string()
    } else {
        format!("refs/heads/{refname}")
    };
    refs.iter()
        .find(|r| r.name == want || r.name.ends_with(&format!("/{refname}")) || r.oid == refname)
        .map(|r| r.oid.clone())
        .or_else(|| {
            // Allow full SHA if present via show_commit later — try exact oid match length.
            if refname.len() >= 7 && refname.chars().all(|c| c.is_ascii_hexdigit()) {
                Some(refname.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| AppError::new("pull.ref_not_found", format!("ref not found: {refname}")))
}

pub struct PullEnrichment {
    usernames: std::collections::HashMap<String, String>,
    head_repos: std::collections::HashMap<String, oxidean_db::RepositoryRow>,
    org_slugs: std::collections::HashMap<String, String>,
    assignees: std::collections::HashMap<String, Vec<oxidean_db::pulls::PullAssigneeRow>>,
}

pub async fn load_pull_enrichment(
    ctx: &RpcCtx,
    rows: &[PullRow],
) -> Result<PullEnrichment, AppError> {
    use std::collections::HashMap;
    if rows.is_empty() {
        return Ok(PullEnrichment {
            usernames: HashMap::new(),
            head_repos: HashMap::new(),
            org_slugs: HashMap::new(),
            assignees: HashMap::new(),
        });
    }
    let pull_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let head_repo_ids: Vec<String> = {
        let mut v: Vec<String> = rows.iter().map(|r| r.head_repo_id.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let (head_repos, assignee_pairs) = tokio::try_join!(
        ctx.db.find_repositories_by_ids(&head_repo_ids),
        ctx.db.list_pull_assignees_for_pulls(&pull_ids),
    )
    .map_err(db_err)?;

    // Authors + user-type head owners share one users lookup; org owners get their own.
    let mut user_ids: Vec<String> = rows.iter().map(|r| r.author_id.clone()).collect();
    let mut org_ids: Vec<String> = Vec::new();
    for repo in &head_repos {
        match repo.owner_type.as_str() {
            "org" => org_ids.push(repo.owner_id.clone()),
            _ => user_ids.push(repo.owner_id.clone()),
        }
    }
    user_ids.sort();
    user_ids.dedup();
    org_ids.sort();
    org_ids.dedup();
    let (users, orgs) = tokio::try_join!(
        ctx.db.find_users_by_ids(&user_ids),
        ctx.db.find_organizations_by_ids(&org_ids),
    )
    .map_err(db_err)?;

    let mut assignees: HashMap<String, Vec<oxidean_db::pulls::PullAssigneeRow>> = HashMap::new();
    for (pull_id, a) in assignee_pairs {
        assignees.entry(pull_id).or_default().push(a);
    }
    Ok(PullEnrichment {
        usernames: users.into_iter().map(|u| (u.id, u.username)).collect(),
        head_repos: head_repos.into_iter().map(|r| (r.id.clone(), r)).collect(),
        org_slugs: orgs.into_iter().map(|o| (o.id, o.slug)).collect(),
        assignees,
    })
}

pub fn pull_row_to_public(row: &PullRow, enr: &PullEnrichment) -> Result<PullPublic, AppError> {
    let state = PullState::parse(&row.state).map_err(|e| {
        tracing::error!(error = %e, "invalid pull state in db");
        AppError::new("pull.internal", "pull operation failed")
    })?;
    let merge_method = match row.merge_method.as_deref() {
        Some(m) => Some(MergeMethod::parse(m).map_err(|e| {
            tracing::error!(error = %e, "invalid merge method in db");
            AppError::new("pull.internal", "pull operation failed")
        })?),
        None => None,
    };
    let author_username = enr
        .usernames
        .get(&row.author_id)
        .cloned()
        .unwrap_or_default();

    let head_repo = enr
        .head_repos
        .get(&row.head_repo_id)
        .ok_or_else(|| AppError::new("pull.internal", "head repository missing"))?;
    let (head_owner, head_name) = match head_repo.owner_type.as_str() {
        "org" => (
            enr.org_slugs
                .get(&head_repo.owner_id)
                .cloned()
                .unwrap_or_default(),
            head_repo.name.clone(),
        ),
        _ => (
            enr.usernames
                .get(&head_repo.owner_id)
                .cloned()
                .unwrap_or_default(),
            head_repo.name.clone(),
        ),
    };

    let assignees = enr
        .assignees
        .get(&row.id)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|a| IssueAssigneePublic {
            user_id: a.user_id.clone(),
            username: a.username.clone(),
            display_name: a.display_name.clone(),
        })
        .collect();

    Ok(PullPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        number: row.number,
        title: row.title.clone(),
        body: row.body.clone(),
        state,
        draft: row.draft,
        author_id: row.author_id.clone(),
        author_username,
        base_ref: row.base_ref.clone(),
        base_sha: row.base_sha.clone(),
        head_repo_id: row.head_repo_id.clone(),
        head_owner,
        head_name,
        head_ref: row.head_ref.clone(),
        head_sha: row.head_sha.clone(),
        merged_at: row.merged_at.clone(),
        merged_by: row.merged_by.clone(),
        merge_commit_sha: row.merge_commit_sha.clone(),
        merge_method,
        closed_at: row.closed_at.clone(),
        closed_by: row.closed_by.clone(),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
        comment_count: row.comment_count,
        assignees,
    })
}

pub async fn to_public(ctx: &RpcCtx, row: &PullRow) -> Result<PullPublic, AppError> {
    let rows = std::slice::from_ref(row);
    let enrichment = load_pull_enrichment(ctx, rows).await?;
    pull_row_to_public(row, &enrichment)
}

pub async fn to_public_many(ctx: &RpcCtx, rows: &[PullRow]) -> Result<Vec<PullPublic>, AppError> {
    let enrichment = load_pull_enrichment(ctx, rows).await?;
    rows.iter()
        .map(|row| pull_row_to_public(row, &enrichment))
        .collect()
}

pub async fn load_pull_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    number: i64,
) -> Result<PullRow, AppError> {
    if number < 1 {
        return Err(pull_not_found());
    }
    ctx.db
        .find_pull_by_repo_number(repo_id, number)
        .await
        .map_err(db_err)?
        .ok_or_else(pull_not_found)
}

/// `pull.create` — Read+ on base; Write+ on the head repo (same-repo head ⇒ Write+ on
/// base; fork head ⇒ the caller owns the branch). Shared `#N` with issues (D-PR-02 / D-PR-29).
pub async fn create(ctx: &RpcCtx, input: serde_json::Value) -> Result<PullPublic, AppError> {
    let user = require_verified(ctx).await?;
    let req: CreatePullRequest = serde_json::from_value(input)
        .map_err(|e| AppError::new("rpc.bad_input", format!("invalid pull.create input: {e}")))?;
    let title = validate_title(&req.title)?.to_string();
    let body = validate_body(req.body.as_deref())?;
    let base_ref = validate_ref_name(&req.base_ref, "base_ref")?;
    let head_ref = validate_ref_name(&req.head_ref, "head_ref")?;
    let draft = req.draft.unwrap_or(false);

    let accessible = acl::resolve_for_read(ctx, &req.owner, &req.name).await?;
    // GIT-20: archived base repos reject new PRs (read-only).
    crate::repo::ensure_not_archived(&accessible)?;

    let head_owner = req
        .head_owner
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(accessible.owner_username.as_str())
        .to_string();
    let head_name = req
        .head_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(accessible.row.name.as_str())
        .to_string();

    let (head_repo_id, head_owner_slug, head_repo_name) = if head_owner == accessible.owner_username
        && head_name == accessible.row.name
    {
        if !crate::repo::meets(accessible.capability, crate::repo::Capability::Write) {
            return Err(crate::repo::not_found());
        }
        (
            accessible.row.id.clone(),
            accessible.owner_username.clone(),
            accessible.row.name.clone(),
        )
    } else {
        let head = crate::repo::resolve_repo_for_read(ctx, &head_owner, &head_name).await?;
        if !crate::repo::meets(head.capability, crate::repo::Capability::Write) {
            return Err(crate::repo::not_found());
        }
        let network = ctx
            .db
            .get_repo_fork_network_id(&head.row.id)
            .await
            .map_err(db_err)?;
        if !crate::repo::head_valid_for_base(&accessible.row.id, &head.row.id, network.as_deref()) {
            return Err(AppError::new(
                "pull.invalid_head",
                "head repository must be this repo or a fork of it",
            ));
        }
        (
            head.row.id.clone(),
            head.owner_username.clone(),
            head.row.name.clone(),
        )
    };

    let base_sha = resolve_ref_sha(
        ctx,
        &accessible.owner_username,
        &accessible.row.name,
        &base_ref,
    )
    .await?;
    let head_sha = resolve_ref_sha(ctx, &head_owner_slug, &head_repo_name, &head_ref).await?;

    if base_ref == head_ref && accessible.row.id == head_repo_id && base_sha == head_sha {
        return Err(AppError::new(
            "rpc.bad_input",
            "base and head refs are identical",
        ));
    }

    let number = ctx
        .db
        .allocate_next_issue_number(&accessible.row.id)
        .await
        .map_err(db_err)?;
    let id = Uuid::new_v4().to_string();
    let row = ctx
        .db
        .insert_pull(
            &id,
            &accessible.row.id,
            number,
            &title,
            &body,
            &user.id,
            &base_ref,
            &base_sha,
            &head_repo_id,
            &head_ref,
            &head_sha,
            draft,
        )
        .await
        .map_err(db_err)?;
    // API-06: synthesize refs/pull/{N}/head so upload-pack advertises it.
    refs::sync_head_ref(
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &ctx.db,
        &accessible.owner_username,
        &accessible.row.name,
        &row,
    )
    .await;
    let subject = notify::subject_for_pull(&row);
    let mentions = notify::resolve_mention_user_ids(&ctx.db, &body).await;
    let requested = ctx
        .db
        .list_pull_review_request_user_ids(&row.id)
        .await
        .unwrap_or_default();
    let mut recipients = requested;
    for m in &mentions {
        if !recipients.iter().any(|r| r == m) {
            recipients.push(m.clone());
        }
    }
    // One watch-level lookup serves both fanouts below.
    let watch = ctx.db.list_repo_watch_levels(&subject.repo_id).await;
    notify::fanout_activity_with_watch(
        &ctx.db,
        &user.id,
        recipients.clone(),
        "pr_opened",
        &subject,
        &watch,
    )
    .await;
    notify::fanout_suppress_ignored_with_watch(
        &ctx.db,
        &user.id,
        mentions,
        "pr_mention",
        &subject,
        &watch,
    )
    .await;
    emit_pull_event(
        ctx,
        &accessible,
        &row,
        "opened",
        &user.username,
        &user.id,
        false,
    )
    .await;
    notify_actions_for_pull(
        &ctx.db,
        ctx.git.as_ref(),
        &ctx.repos_dir,
        &row,
        &accessible.owner_username,
        &accessible.row.name,
        &head_owner_slug,
        &head_repo_name,
        Some(&user.id),
        crate::actions::events::rpc_caller_is_job_token(ctx),
        crate::actions::PullRequestAction::Opened,
        false,
    )
    .await;
    to_public(ctx, &row).await
}
