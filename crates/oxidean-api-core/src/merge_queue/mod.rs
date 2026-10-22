//! Shared machinery retained in core after the RPC handlers moved to
//! `oxidean-api-merge-queue` (#162).

use crate::git::bare_repo_path;
use crate::repo::{ensure_not_archived, meets, resolve_repo_for_read, AccessibleRepo, Capability};
use crate::rpc::RpcCtx;
use crate::webhook::dispatch;
use oxidean_core::AppError;
use oxidean_db::{Database, MergeGroupRow, MergeQueueEntryRow, PullRow, UserRow};
use oxidean_git::GitBackend;
use std::path::PathBuf;
use std::sync::Arc;

pub fn db_err(e: String) -> AppError {
    tracing::error!(error = %e, "merge_queue db error");
    AppError::new("merge_queue.internal", "merge queue operation failed")
}

pub fn invalid(message: &str) -> AppError {
    AppError::new("merge_queue.invalid", message)
}

pub fn conflict(message: &str) -> AppError {
    AppError::new("merge_queue.conflict", message)
}

/// GitHub names the queue's verification ref `gh-readonly-queue/…` — kept
/// verbatim so `github.ref` on `merge_group` runs matches upstream.
pub fn group_head_ref(base_ref: &str, group_id: &str) -> String {
    format!("refs/heads/gh-readonly-queue/{base_ref}/group-{group_id}")
}

/// Everything the queue orchestrator needs — `MqDeps::from_ctx` for RPC
/// handlers; check/post-receive seams build it field-by-field.
#[derive(Clone)]
pub struct MqDeps {
    pub db: Database,
    pub git: Arc<dyn GitBackend>,
    pub repos_dir: PathBuf,
    pub pages_dir: PathBuf,
    pub env_name: String,
    pub job_token_actor: bool,
    pub actions_enabled: bool,
}

impl MqDeps {
    pub fn from_ctx(ctx: &RpcCtx) -> Self {
        Self {
            db: ctx.db.clone(),
            git: ctx.git.clone(),
            repos_dir: ctx.repos_dir.clone(),
            pages_dir: ctx.pages_dir.clone(),
            env_name: ctx.env_name.clone(),
            job_token_actor: crate::actions::events::rpc_caller_is_job_token(ctx),
            actions_enabled: crate::actions::env_actions_enabled(),
        }
    }
}

/// Narrow repo scope for non-RPC seams (check completion, post-receive,
/// pull sync) — the `AccessibleRepo` fields the orchestrator touches.
pub struct QueueScope {
    owner: String,
    name: String,
    default_branch: String,
    repo_id: String,
}

impl QueueScope {
    pub fn of(accessible: &AccessibleRepo) -> Self {
        Self {
            owner: accessible.owner_username.clone(),
            name: accessible.row.name.clone(),
            default_branch: accessible.row.default_branch.clone(),
            repo_id: accessible.row.id.clone(),
        }
    }
}

/// Repo coords for db-only seams — mirrors `emit_pull_event_db`'s owner
/// resolution (user → username, org → slug).
pub async fn repo_coords(db: &Database, repo_id: &str) -> Option<QueueScope> {
    let repo = db.find_repository_by_id(repo_id).await.ok()??;
    let owner = match repo.owner_type.as_str() {
        "org" => db
            .find_organization_by_id(&repo.owner_id)
            .await
            .ok()?
            .map(|o| o.slug)
            .unwrap_or_default(),
        _ => db
            .find_user_by_id(&repo.owner_id)
            .await
            .ok()?
            .map(|u| u.username)
            .unwrap_or_default(),
    };
    Some(QueueScope {
        owner,
        name: repo.name,
        default_branch: repo.default_branch,
        repo_id: repo_id.to_string(),
    })
}

pub fn entry_to_public(
    row: &MergeQueueEntryRow,
    pull: Option<&PullRow>,
    enqueued_by_username: Option<String>,
) -> oxidean_core::MergeQueueEntryPublic {
    oxidean_core::MergeQueueEntryPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        pull_number: row.pull_number,
        base_ref: row.base_ref.clone(),
        position: row.position,
        state: row.state.clone(),
        group_id: row.group_id.clone(),
        enqueued_by: (!row.enqueued_by.is_empty()).then(|| row.enqueued_by.clone()),
        enqueued_by_username,
        dequeue_reason: row.dequeue_reason.clone(),
        pull: pull.map(|p| oxidean_core::MergeQueuePullEmbed {
            number: p.number,
            title: p.title.clone(),
            state: p.state.clone(),
            head_ref: p.head_ref.clone(),
            head_sha: p.head_sha.clone(),
            base_ref: p.base_ref.clone(),
        }),
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

pub async fn group_to_public(db: &Database, row: &MergeGroupRow) -> oxidean_core::MergeGroupPublic {
    let pull_numbers = db
        .list_merge_queue_entries_for_group(&row.id)
        .await
        .unwrap_or_default()
        .iter()
        .map(|e| e.pull_number)
        .collect();
    oxidean_core::MergeGroupPublic {
        id: row.id.clone(),
        repo_id: row.repo_id.clone(),
        base_ref: row.base_ref.clone(),
        base_sha: row.base_sha.clone(),
        head_ref: row.head_ref.clone(),
        head_sha: row.head_sha.clone(),
        state: row.state.clone(),
        pull_numbers,
        created_at: row.created_at.clone(),
        updated_at: row.updated_at.clone(),
    }
}

pub async fn load_pull_for_entry(db: &Database, entry: &MergeQueueEntryRow) -> Option<PullRow> {
    db.find_pull_by_id(&entry.pull_id).await.ok().flatten()
}

pub async fn username_of(db: &Database, user_id: &str) -> Option<String> {
    if user_id.is_empty() {
        return None;
    }
    db.find_user_by_id(user_id).await.ok()?.map(|u| u.username)
}

pub async fn resolve_capability(
    ctx: &RpcCtx,
    owner: &str,
    name: &str,
    need: Capability,
) -> Result<AccessibleRepo, AppError> {
    let accessible = resolve_repo_for_read(ctx, owner, name).await?;
    if !meets(accessible.capability, need) {
        return Err(crate::repo::not_found());
    }
    ensure_not_archived(&accessible)?;
    Ok(accessible)
}

pub async fn load_pull_in_repo(
    ctx: &RpcCtx,
    repo_id: &str,
    number: i64,
) -> Result<PullRow, AppError> {
    ctx.db
        .find_pull_by_repo_number(repo_id, number)
        .await
        .map_err(db_err)?
        .ok_or_else(crate::repo::not_found)
}

/// `pull_request` webhook + lifecycle for queue membership changes —
/// `enqueued`/`dequeued` activity types (GitHub emits both).
pub async fn emit_pull_queue_event(
    deps: &MqDeps,
    scope: &QueueScope,
    pull: &PullRow,
    action: &str,
    dequeue_reason: Option<&str>,
    sender_login: &str,
    sender_id: &str,
) {
    crate::pull::emit_pull_event_db(
        &deps.db,
        &scope.owner,
        &scope.name,
        &scope.repo_id,
        pull,
        action,
        sender_login,
        sender_id,
        false,
        &deps.env_name,
        dequeue_reason,
    )
    .await;
    let (head_owner, head_name) = if pull.head_repo_id == pull.repo_id {
        (scope.owner.clone(), scope.name.clone())
    } else {
        repo_coords(&deps.db, &pull.head_repo_id)
            .await
            .map(|s| (s.owner, s.name))
            .unwrap_or_else(|| (scope.owner.clone(), scope.name.clone()))
    };
    let pr_action = if action == "enqueued" {
        crate::actions::PullRequestAction::Enqueued
    } else {
        crate::actions::PullRequestAction::Dequeued
    };
    crate::pull::notify_actions_for_pull(
        &deps.db,
        deps.git.as_ref(),
        &deps.repos_dir,
        pull,
        &scope.owner,
        &scope.name,
        &head_owner,
        &head_name,
        (!sender_id.is_empty()).then_some(sender_id),
        deps.job_token_actor,
        pr_action,
        false,
    )
    .await;
}

/// Form the next `checking` group for a train — no-op while one is already
/// checking (entries wait; they join the *next* group, GitHub parity) or
/// when nothing is queued. Returns the formed group.
pub async fn form_group(
    deps: &MqDeps,
    scope: &QueueScope,
    base_ref: &str,
    actor: Option<&UserRow>,
) -> Result<Option<MergeGroupRow>, AppError> {
    if deps
        .db
        .find_checking_merge_group(&scope.repo_id, base_ref)
        .await
        .map_err(db_err)?
        .is_some()
    {
        return Ok(None);
    }
    let pending: Vec<MergeQueueEntryRow> = deps
        .db
        .list_merge_queue_entries(&scope.repo_id, base_ref, "queued")
        .await
        .map_err(db_err)?
        .into_iter()
        .filter(|e| e.group_id.is_none())
        .collect();
    if pending.is_empty() {
        return Ok(None);
    }
    let bare = bare_repo_path(&deps.repos_dir, &scope.owner, &scope.name)?;
    let base_sha = deps
        .git
        .rev_parse(&bare, &format!("refs/heads/{base_ref}"))
        .await
        .map_err(|_| invalid("base branch does not resolve"))?;

    // Build the merge-commit chain; entries that no longer merge (closed
    // PRs, mid-train conflicts) dequeue — GitHub drops unmergeable PRs
    // from the group rather than wedging the train.
    let mut chain_tip = base_sha.clone();
    let mut survivors: Vec<(MergeQueueEntryRow, PullRow, String)> = Vec::new();
    for entry in pending {
        let Some(pull) = load_pull_for_entry(&deps.db, &entry).await else {
            deps.db
                .set_merge_queue_entry_state(&entry.id, "dequeued", "closed")
                .await
                .map_err(db_err)?;
            continue;
        };
        if pull.state != "open" {
            deps.db
                .set_merge_queue_entry_state(&entry.id, "dequeued", "closed")
                .await
                .map_err(db_err)?;
            continue;
        }
        if let Err(e) = crate::pull::ensure_head_objects_in_base(
            &deps.db,
            deps.git.as_ref(),
            &deps.repos_dir,
            &bare,
            &pull,
        )
        .await
        {
            tracing::warn!(error = %e.message, pull = pull.number, "merge queue: head fetch failed");
            deps.db
                .set_merge_queue_entry_state(&entry.id, "dequeued", "merge_conflict")
                .await
                .map_err(db_err)?;
            continue;
        }
        let message = format!(
            "Merge pull request #{} from {}\n\n{}",
            pull.number, pull.head_ref, pull.title
        );
        match deps
            .git
            .test_merge_commit(&bare, &chain_tip, &pull.head_sha, &message)
            .await
        {
            Ok(sha) => {
                chain_tip = sha.clone();
                survivors.push((entry, pull, sha));
            }
            Err(e) => {
                tracing::warn!(error = %e, pull = pull.number, "merge queue: train merge conflict");
                deps.db
                    .set_merge_queue_entry_state(&entry.id, "dequeued", "merge_conflict")
                    .await
                    .map_err(db_err)?;
            }
        }
    }
    if survivors.is_empty() {
        return Ok(None);
    }

    let group_id = uuid::Uuid::new_v4().to_string();
    let head_ref = group_head_ref(base_ref, &group_id);
    // The row inserts before the ref publishes: the checking-per-base
    // unique index resolves concurrent `form_group` calls here — the
    // loser re-checks and rides the winner's group (its own ref is
    // never written, so no orphan refs).
    let group = match deps
        .db
        .insert_merge_group(
            &group_id,
            &scope.repo_id,
            base_ref,
            &base_sha,
            &head_ref,
            &chain_tip,
        )
        .await
    {
        Ok(g) => g,
        Err(e) => {
            if deps
                .db
                .find_checking_merge_group(&scope.repo_id, base_ref)
                .await
                .map_err(db_err)?
                .is_some()
            {
                return Ok(None);
            }
            return Err(db_err(e));
        }
    };
    if let Err(e) = deps.git.update_ref(&bare, &head_ref, &chain_tip).await {
        let _ = deps.db.set_merge_group_state(&group.id, "destroyed").await;
        return Err(invalid(&format!("publish queue head failed: {e}")));
    }
    let mut pull_numbers = Vec::with_capacity(survivors.len());
    for (entry, pull, chain_sha) in &survivors {
        deps.db
            .set_entry_group_and_chain(&entry.id, Some(&group_id), chain_sha)
            .await
            .map_err(db_err)?;
        pull_numbers.push(pull.number);
    }

    emit_merge_group(
        deps,
        scope,
        &group,
        "checks_requested",
        &pull_numbers,
        actor,
    )
    .await;
    Ok(Some(group))
}

/// `merge_group` webhook + `on: merge_group` lifecycle. `checks_requested`
/// is the only type that starts runs; `destroyed` is webhook-only upstream.
pub async fn emit_merge_group(
    deps: &MqDeps,
    scope: &QueueScope,
    group: &MergeGroupRow,
    action: &str,
    pull_numbers: &[i64],
    actor: Option<&UserRow>,
) {
    let repository =
        crate::actions::payload::repository_embed(&scope.owner, &scope.name, &scope.default_branch);
    let sender = crate::actions::payload::user_embed(
        actor.map(|u| u.username.as_str()).unwrap_or("oxidean[bot]"),
    );
    let bare = match bare_repo_path(&deps.repos_dir, &scope.owner, &scope.name) {
        Ok(p) => p,
        Err(_) => return,
    };
    let head_commit = match deps
        .git
        .show_commit(&bare, &group.head_sha, None, None)
        .await
    {
        Ok(c) => serde_json::json!({
            "id": group.head_sha,
            "message": format!("{}\n{}", c.subject, c.body).trim_end(),
            "timestamp": c.authored_at,
            "author": {"name": c.author_name, "email": c.author_email},
            "committer": {"name": c.author_name, "email": c.committer_email},
        }),
        Err(_) => serde_json::json!({"id": group.head_sha}),
    };
    let payload = crate::actions::payload::merge_group_payload(
        action,
        group,
        &head_commit,
        pull_numbers,
        &repository,
        &sender,
    );
    dispatch::emit(
        &deps.db,
        &scope.repo_id,
        "merge_group",
        action,
        payload.clone(),
        &deps.env_name,
    )
    .await;
    if action != "checks_requested" {
        return;
    }
    crate::actions::dispatch::notify_merge_group_actions(
        &deps.db,
        deps.git.as_ref(),
        &bare,
        &scope.repo_id,
        &group.head_sha,
        &group.head_ref,
        &payload,
        actor.map(|u| u.id.as_str()),
        deps.job_token_actor,
        deps.actions_enabled,
    )
    .await;
}

/// Remove one entry from the train — marks `dequeued`, emits
/// `pull_request` `dequeued`, and when the entry rode the `checking` group
/// destroys it; remaining members stay queued and re-form.
pub async fn remove_entry(
    deps: &MqDeps,
    scope: &QueueScope,
    entry: &MergeQueueEntryRow,
    reason: &str,
    sender_login: &str,
    sender_id: &str,
) -> Result<MergeQueueEntryRow, AppError> {
    let rode_group = entry.group_id.clone();
    let entry = deps
        .db
        .set_merge_queue_entry_state(&entry.id, "dequeued", reason)
        .await
        .map_err(db_err)?
        .ok_or_else(|| AppError::new("merge_queue.internal", "entry vanished"))?;
    if let Some(pull) = load_pull_for_entry(&deps.db, &entry).await {
        emit_pull_queue_event(
            deps,
            scope,
            &pull,
            "dequeued",
            Some(&entry.dequeue_reason),
            sender_login,
            sender_id,
        )
        .await;
    }
    if let Some(gid) = rode_group {
        if let Some(group) = deps.db.find_merge_group(&gid).await.map_err(db_err)? {
            if group.state == "checking" {
                destroy_group(deps, scope, &group, true).await?;
            }
        }
    }
    Ok(entry)
}

/// Teardown for a `checking` group — marks it `destroyed`, emits the
/// `merge_group` webhook, then either dequeues every member
/// (`keep_entries = false`, group verification failed) or clears them back
/// to unassigned `queued` (`keep_entries = true` — one entry left, or the
/// base moved). The train re-forms afterwards either way.
pub async fn destroy_group(
    deps: &MqDeps,
    scope: &QueueScope,
    group: &MergeGroupRow,
    keep_entries: bool,
) -> Result<(), AppError> {
    let entries = deps
        .db
        .list_merge_queue_entries_for_group(&group.id)
        .await
        .map_err(db_err)?;
    let pull_numbers: Vec<i64> = entries.iter().map(|e| e.pull_number).collect();
    deps.db
        .set_merge_group_state(&group.id, "destroyed")
        .await
        .map_err(db_err)?;
    emit_merge_group(deps, scope, group, "destroyed", &pull_numbers, None).await;
    for entry in entries {
        if keep_entries {
            deps.db
                .set_entry_group_and_chain(&entry.id, None, "")
                .await
                .map_err(db_err)?;
        } else {
            deps.db
                .set_merge_queue_entry_state(&entry.id, "dequeued", "failed_checks")
                .await
                .map_err(db_err)?;
            if let Some(pull) = load_pull_for_entry(&deps.db, &entry).await {
                emit_pull_queue_event(
                    deps,
                    scope,
                    &pull,
                    "dequeued",
                    Some("failed_checks"),
                    "oxidean[bot]",
                    "",
                )
                .await;
            }
        }
    }
    form_group(deps, scope, &group.base_ref, None).await?;
    Ok(())
}

/// Land a verified group: fast-forward `base_ref` to the tested chain tip,
/// mark each PR merged on its chain link, `pull_request` `closed`
/// (merged=true), then replay the push fan-out for the branch move —
/// ref events, open-PR synchronize, pages rebuild, `push` workflow runs.
pub async fn land_group(
    deps: &MqDeps,
    scope: &QueueScope,
    group: &MergeGroupRow,
) -> Result<(), AppError> {
    let bare = bare_repo_path(&deps.repos_dir, &scope.owner, &scope.name)?;
    let base_tip = deps
        .git
        .rev_parse(&bare, &format!("refs/heads/{}", group.base_ref))
        .await
        .unwrap_or_default();
    if base_tip != group.base_sha {
        // The base moved under the group (external push or a prior
        // landing) — the verified tree is stale. GitHub destroys and
        // re-forms on the new tip; entries stay queued.
        return destroy_group(deps, scope, group, true).await;
    }
    deps.git
        .update_ref(
            &bare,
            &format!("refs/heads/{}", group.base_ref),
            &group.head_sha,
        )
        .await
        .map_err(|e| invalid(&format!("queue landing failed: {e}")))?;
    deps.db
        .set_merge_group_state(&group.id, "merged")
        .await
        .map_err(db_err)?;

    let entries = deps
        .db
        .list_merge_queue_entries_for_group(&group.id)
        .await
        .map_err(db_err)?;
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    for entry in &entries {
        let merge_sha = if entry.chain_sha.is_empty() {
            group.head_sha.as_str()
        } else {
            entry.chain_sha.as_str()
        };
        deps.db
            .set_merge_queue_entry_state(&entry.id, "merged", "")
            .await
            .map_err(db_err)?;
        let Some(pull) = load_pull_for_entry(&deps.db, entry).await else {
            continue;
        };
        let merger = username_of(&deps.db, &entry.enqueued_by)
            .await
            .unwrap_or_else(|| "oxidean[bot]".into());
        deps.db
            .mark_pull_merged(&pull.id, &entry.enqueued_by, merge_sha, "merge", &now)
            .await
            .map_err(db_err)?;
        crate::pull::refs::sync_merge_ref(
            deps.git.as_ref(),
            &deps.repos_dir,
            &scope.owner,
            &scope.name,
            pull.number,
            merge_sha,
        )
        .await;
        let subject = format!("{} (#{})", pull.title, pull.number);
        crate::repo::record_pr_merge(
            &deps.db,
            &scope.repo_id,
            &entry.enqueued_by,
            &group.base_ref,
            merge_sha,
            pull.number,
            Some(&subject),
        )
        .await;
        crate::pull::emit_pull_event_db(
            &deps.db,
            &scope.owner,
            &scope.name,
            &scope.repo_id,
            &pull,
            "closed",
            &merger,
            &entry.enqueued_by,
            true,
            &deps.env_name,
            None,
        )
        .await;
        let (head_owner, head_name) = if pull.head_repo_id == pull.repo_id {
            (scope.owner.clone(), scope.name.clone())
        } else {
            repo_coords(&deps.db, &pull.head_repo_id)
                .await
                .map(|s| (s.owner, s.name))
                .unwrap_or_else(|| (scope.owner.clone(), scope.name.clone()))
        };
        crate::pull::notify_actions_for_pull(
            &deps.db,
            deps.git.as_ref(),
            &deps.repos_dir,
            &pull,
            &scope.owner,
            &scope.name,
            &head_owner,
            &head_name,
            Some(&entry.enqueued_by),
            deps.job_token_actor,
            crate::actions::PullRequestAction::Closed,
            true,
        )
        .await;
    }

    // The branch just moved — replay the push fan-out a git receive would
    // have produced (API-04 ref events, open-PR base sync, pages rebuild,
    // `push` runs, mirrors). The pusher of record is the enqueuer of the
    // train's last PR (GitHub attributes the queue merge to the merge
    // queue, sender = enqueuer).
    let pusher_id = entries
        .last()
        .map(|e| e.enqueued_by.clone())
        .unwrap_or_default();
    let pusher_login = username_of(&deps.db, &pusher_id)
        .await
        .unwrap_or_else(|| "oxidean[bot]".into());
    let updates = vec![(
        group.base_sha.clone(),
        group.head_sha.clone(),
        format!("refs/heads/{}", group.base_ref),
    )];
    crate::webhook::dispatch::notify_ref_events(
        &deps.db,
        &scope.repo_id,
        &scope.owner,
        &scope.name,
        &pusher_login,
        &pusher_id,
        &updates,
        &deps.env_name,
        deps.git.as_ref(),
        &bare,
        false,
    )
    .await;
    crate::pull::synchronize_after_push(
        &deps.db,
        &deps.repos_dir,
        &scope.repo_id,
        &scope.owner,
        &scope.name,
        &pusher_login,
        &pusher_id,
        &updates,
        &deps.env_name,
        false,
    )
    .await;
    crate::pages::rebuild_after_push(
        &deps.db,
        deps.git.as_ref(),
        &deps.repos_dir,
        &deps.pages_dir,
        &deps.env_name,
        &scope.repo_id,
        &scope.owner,
        &pusher_id,
        &updates,
        false,
        deps.actions_enabled,
    )
    .await;
    crate::actions::notify_push_actions(
        &deps.db,
        deps.git.clone(),
        &deps.repos_dir,
        &scope.repo_id,
        &scope.owner,
        &scope.name,
        (!pusher_id.is_empty()).then_some(pusher_id.as_str()),
        &updates,
        deps.actions_enabled,
    )
    .await;
    crate::mirror::notify_mirror_after_local_mutation(
        deps.db.clone(),
        deps.git.clone(),
        deps.repos_dir.clone(),
        scope.repo_id.clone(),
    );
    Ok(())
}

/// Check-suite completion seam — [`crate::check`] calls this whenever a
/// suite rolls `completed`. A `checking` group resolves only once *every*
/// suite on its head finishes (GitHub requires the whole group check set):
/// all `success`/`skipped` lands the train (base fast-forwards to the
/// verified chain — the tested commits become history); any other
/// conclusion destroys the group and dequeues its entries `failed_checks`.
pub async fn after_suite_completed(
    deps: &MqDeps,
    suite_repo_id: &str,
    suite_head_sha: &str,
    suite_conclusion: &str,
) {
    let Some(group) = deps
        .db
        .find_checking_merge_group_for_head(suite_repo_id, suite_head_sha)
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    let Some(scope) = repo_coords(&deps.db, suite_repo_id).await else {
        return;
    };
    let suites = deps
        .db
        .list_check_suites(suite_repo_id, Some(suite_head_sha), 50)
        .await
        .unwrap_or_default();
    // A failing suite decides immediately; otherwise the group waits for
    // every suite on the head to report in.
    let failed_now = suite_conclusion != "success" && suite_conclusion != "skipped";
    if !failed_now && suites.iter().any(|s| s.status != "completed") {
        return;
    }
    let success = !failed_now
        && !suites.is_empty()
        && suites
            .iter()
            .all(|s| matches!(s.conclusion.as_deref(), Some("success") | Some("skipped")));
    let result = if success {
        land_group(deps, &scope, &group).await
    } else {
        destroy_group(deps, &scope, &group, false).await
    };
    if let Err(e) = result {
        tracing::warn!(
            error = %e.message,
            head_sha = suite_head_sha,
            "merge queue group resolution failed (soft-fail)"
        );
    }
}

/// Post-receive seam: a push that moved a queued train's base branch
/// invalidates the `checking` group on it — destroy + re-form (entries
/// stay queued; GitHub restarts verification on the new tip).
pub async fn after_base_pushed(deps: &MqDeps, repo_id: &str, base_ref: &str) {
    let landed = async {
        let Some(group) = deps
            .db
            .find_checking_merge_group(repo_id, base_ref)
            .await
            .map_err(db_err)?
        else {
            return Ok(());
        };
        let Some(scope) = repo_coords(&deps.db, repo_id).await else {
            return Ok(());
        };
        destroy_group(deps, &scope, &group, true).await
    }
    .await;
    if let Err(e) = landed {
        tracing::warn!(
            error = %e.message,
            repo_id,
            base_ref,
            "merge queue base-push invalidation failed (soft-fail)"
        );
    }
}

/// PR head moved (`synchronize_pull_after_head_move` calls this after the
/// stored head updates) — the queued commit is stale; GitHub removes the
/// PR from the queue.
pub async fn after_pull_head_moved(
    deps: &MqDeps,
    repo_id: &str,
    pull: &PullRow,
    sender_login: &str,
    sender_id: &str,
) {
    dequeue_entry_for_pull(deps, repo_id, pull, "head_changed", sender_login, sender_id).await;
}

/// PR closed without merging — GitHub removes it from the queue.
pub async fn after_pull_closed(
    deps: &MqDeps,
    repo_id: &str,
    pull: &PullRow,
    sender_login: &str,
    sender_id: &str,
) {
    dequeue_entry_for_pull(deps, repo_id, pull, "closed", sender_login, sender_id).await;
}

/// Dequeue a pull's live entry — shared by every auto-remove reason
/// (`head_moved`, `closed`, `retargeted`). Emits `pull_request` `dequeued`
/// and tears down the riding `checking` group (members stay queued).
pub async fn dequeue_entry_for_pull(
    deps: &MqDeps,
    repo_id: &str,
    pull: &PullRow,
    reason: &str,
    sender_login: &str,
    sender_id: &str,
) {
    let result: Result<(), AppError> = async {
        let Some(entry) = deps
            .db
            .find_queued_entry_for_pull(&pull.id)
            .await
            .map_err(db_err)?
        else {
            return Ok(());
        };
        let Some(scope) = repo_coords(&deps.db, repo_id).await else {
            return Ok(());
        };
        let rode_group = entry.group_id.clone();
        deps.db
            .set_merge_queue_entry_state(&entry.id, "dequeued", reason)
            .await
            .map_err(db_err)?;
        emit_pull_queue_event(
            deps,
            &scope,
            pull,
            "dequeued",
            Some(reason),
            sender_login,
            sender_id,
        )
        .await;
        if let Some(gid) = rode_group {
            if let Some(group) = deps.db.find_merge_group(&gid).await.map_err(db_err)? {
                if group.state == "checking" {
                    destroy_group(deps, &scope, &group, true).await?;
                }
            }
        }
        Ok(())
    }
    .await;
    if let Err(e) = result {
        tracing::warn!(
            error = %e.message,
            pull = pull.number,
            reason,
            "merge queue auto-dequeue failed (soft-fail)"
        );
    }
}
