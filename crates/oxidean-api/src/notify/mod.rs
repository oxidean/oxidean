//! Best-effort notification fan-out after domain writes (NOTF-01 / D-03).
//!
//! Access-loss pruning (DEBT-06): when a user can no longer read a repository
//! their watch row is removed (GitHub auto-unwatch) and its notification rows
//! are deleted — eagerly at every ACL mutation point and lazily when the user
//! reads their inbox / watch list.

use std::collections::HashSet;

use oxidean_db::Database;
use uuid::Uuid;

use crate::repo::{effective_capability, meets, owner_ref_for_repo, Capability};

/// Subject metadata for a notification row.
#[derive(Debug, Clone)]
pub struct NotifySubject {
    pub kind: &'static str,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    /// Deep-link ref for non-numbered subjects (release tag, workflow run id).
    pub subject_ref: Option<String>,
}

/// Insert one notification per recipient. Soft-fails: logs errors and does not
/// abort the caller. `exclude_actor` drops the actor from the recipient set
/// (D-03); run-completion fan-out disables it because the triggering user must
/// learn the outcome of their own run.
async fn fanout_inner(
    db: &Database,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
    exclude_actor: bool,
) {
    let mut seen = HashSet::new();
    for recipient_id in recipients {
        if recipient_id.is_empty() || (exclude_actor && recipient_id == actor_id) {
            continue;
        }
        if !seen.insert(recipient_id.clone()) {
            continue;
        }
        let id = Uuid::new_v4().to_string();
        if let Err(e) = db
            .insert_notification(
                &id,
                &recipient_id,
                actor_id,
                reason,
                subject.kind,
                &subject.repo_id,
                subject.number,
                &subject.title,
                subject.subject_ref.as_deref(),
            )
            .await
        {
            tracing::warn!(
                error = %e,
                recipient_id = %recipient_id,
                reason = %reason,
                "notification fanout insert failed (soft-fail)"
            );
        }
    }
}

/// Insert one notification per recipient, excluding the actor (D-03).
/// Soft-fails: logs errors and does not abort the caller.
pub async fn fanout(
    db: &Database,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
) {
    fanout_inner(db, actor_id, recipients, reason, subject, true).await;
}

/// Watch-level-aware fan-out inner: `exclude_actor` controls whether the actor
/// is dropped from the participating set and the watch rows before insertion.
async fn fanout_activity_inner(
    db: &Database,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
    exclude_actor: bool,
) {
    let base: HashSet<String> = recipients
        .into_iter()
        .filter(|r| !r.is_empty() && (!exclude_actor || r != actor_id))
        .collect();
    match db.list_repo_watch_levels(&subject.repo_id).await {
        Ok(rows) => {
            let watched: HashSet<&str> = rows.iter().map(|(uid, _)| uid.as_str()).collect();
            // Non-watchers keep legacy participating/mention delivery.
            let mut out: HashSet<String> = base
                .iter()
                .filter(|uid| !watched.contains(uid.as_str()))
                .cloned()
                .collect();
            for (uid, level) in &rows {
                let include = match oxidean_core::WatchLevel::parse(level) {
                    Ok(oxidean_core::WatchLevel::All) => true,
                    Ok(oxidean_core::WatchLevel::Participating) => base.contains(uid),
                    Ok(oxidean_core::WatchLevel::Ignore) | Err(_) => false,
                };
                if include {
                    out.insert(uid.clone());
                }
            }
            fanout_inner(db, actor_id, out, reason, subject, exclude_actor).await;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                repo_id = %subject.repo_id,
                "watch-level lookup failed; participant-only fanout"
            );
            fanout_inner(db, actor_id, base, reason, subject, exclude_actor).await;
        }
    }
}

/// Watch-level-aware fan-out for repo activity (DEBT-06): subscription rows on
/// the subject's repo apply the notification matrix —
///   * `all` watchers join the caller-computed participating/mentioned set for
///     every repo activity event;
///   * `participating` watchers receive only when already in that set;
///   * `ignore` watchers are suppressed even when they participate;
///   * users with no subscription row keep legacy participating delivery.
/// Soft-fails like `fanout`: a watch-level lookup error falls back to the
/// participant set so delivery is never lost.
pub async fn fanout_activity(
    db: &Database,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
) {
    fanout_activity_inner(db, actor_id, recipients, reason, subject, true).await;
}

/// [`fanout_activity`] but the actor stays eligible for delivery — used by
/// workflow run completion, where the triggering user must learn the outcome
/// of the run their push started (GitHub ci_activity parity, DEBT-06).
pub async fn fanout_activity_including_actor(
    db: &Database,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
) {
    fanout_activity_inner(db, actor_id, recipients, reason, subject, false).await;
}

/// Suppression-only variant for direct-address notifications (mentions,
/// assignments, review requests): the caller-computed recipients pass through
/// unchanged except watchers at `ignore`, which suppresses even @-mentions
/// (GitHub "Ignore" semantics, DEBT-06).
pub async fn fanout_suppress_ignored(
    db: &Database,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
) {
    let list: Vec<String> = recipients.into_iter().collect();
    match db.list_repo_watch_levels(&subject.repo_id).await {
        Ok(rows) => {
            let ignored: HashSet<&str> = rows
                .iter()
                .filter(|(_, l)| l == "ignore")
                .map(|(uid, _)| uid.as_str())
                .collect();
            fanout(
                db,
                actor_id,
                list.into_iter().filter(|uid| !ignored.contains(uid.as_str())),
                reason,
                subject,
            )
            .await;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                repo_id = %subject.repo_id,
                "watch-level lookup failed; unfiltered fanout"
            );
            fanout(db, actor_id, list, reason, subject).await;
        }
    }
}

/// Workflow-run completion fan-out (DEBT-06 follow-up): emits once per run via
/// the `completion_notified` claim flag, routes through the watch matrix, and
/// keeps the triggering user eligible so they learn their own run's outcome.
pub async fn fanout_workflow_completed(db: &Database, run: &oxidean_db::ActionRunRow) {
    let reason = match run.status.as_str() {
        "success" => "workflow_run_success",
        "failure" => "workflow_run_failure",
        "cancelled" => "workflow_run_cancelled",
        other => {
            tracing::warn!(
                run_id = %run.id,
                status = %other,
                "workflow completion notification skipped: unexpected run status"
            );
            return;
        }
    };
    let Some(actor) = resolve_run_actor(db, run).await else {
        tracing::warn!(
            run_id = %run.id,
            "workflow completion notification skipped: no attributable actor"
        );
        return;
    };
    let subject = subject_for_workflow_run(run);
    let recipients: Vec<String> = run
        .triggered_by
        .iter()
        .filter(|uid| !uid.is_empty())
        .cloned()
        .collect();
    fanout_activity_including_actor(db, &actor, recipients, reason, &subject).await;
}

/// Actor shown on a workflow-run notification: the triggering user, falling
/// back to the repo's personal owner or first org owner when the trigger was
/// unattributed (e.g. a schedule with no user context).
async fn resolve_run_actor(db: &Database, run: &oxidean_db::ActionRunRow) -> Option<String> {
    if let Some(uid) = &run.triggered_by {
        if !uid.is_empty() {
            return Some(uid.clone());
        }
    }
    let repo = match db.find_repository_by_id(&run.repository_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return None,
        Err(e) => {
            tracing::warn!(error = %e, run_id = %run.id, "run actor repo lookup failed");
            return None;
        }
    };
    if repo.owner_type.eq_ignore_ascii_case("user") {
        return Some(repo.owner_id.clone());
    }
    match db.list_org_members(&repo.owner_id).await {
        Ok(members) => members
            .into_iter()
            .find(|m| m.role.eq_ignore_ascii_case("owner"))
            .map(|m| m.user_id),
        Err(e) => {
            tracing::warn!(error = %e, run_id = %run.id, "run actor org lookup failed");
            None
        }
    }
}

/// Re-check whether `user_id` can still read `repo_id`. Tri-state: `Some` is
/// the verdict, `None` means the lookup failed and callers must not prune on a
/// transient error. A missing/deleted repo counts as unreadable.
async fn repo_can_read(db: &Database, user_id: &str, repo_id: &str) -> Option<bool> {
    let repo = match db.find_repository_by_id(repo_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, repo_id = %repo_id, "notify prune: repo lookup failed");
            return None;
        }
    };
    let Some(repo) = repo else {
        return Some(false);
    };
    let owner = match owner_ref_for_repo(db, &repo).await {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!(error = %e, repo_id = %repo_id, "notify prune: owner lookup failed");
            return None;
        }
    };
    let Some(owner) = owner else {
        return Some(false);
    };
    match effective_capability(db, Some(user_id), &repo, &owner).await {
        Ok(cap) => Some(meets(cap, Capability::Read)),
        Err(e) => {
            tracing::warn!(
                error = %e,
                repo_id = %repo_id,
                "notify prune: capability check failed"
            );
            None
        }
    }
}

/// Drop one user's watch row and notification rows for a repo they can no
/// longer read (GitHub auto-unwatch on access loss, DEBT-06). Soft-fails.
pub async fn prune_if_repo_read_lost(db: &Database, user_id: &str, repo_id: &str) {
    match repo_can_read(db, user_id, repo_id).await {
        Some(false) => {}
        Some(true) | None => return,
    }
    if let Err(e) = db.unwatch_repository(user_id, repo_id).await {
        tracing::warn!(
            error = %e,
            user_id = %user_id,
            repo_id = %repo_id,
            "auto-unwatch on access loss failed"
        );
    }
    if let Err(e) = db
        .delete_notifications_for_repo_recipient(user_id, repo_id)
        .await
    {
        tracing::warn!(
            error = %e,
            user_id = %user_id,
            repo_id = %repo_id,
            "notification prune on access loss failed"
        );
    }
}

/// Repo-wide re-check after an ACL change that can revoke read for many users
/// (visibility flip, transfer, soft-delete, member-base change). Every watcher
/// and notification recipient is re-validated. Soft-fails.
pub async fn sweep_repo_access(db: &Database, repo_id: &str) {
    let mut candidates: HashSet<String> = HashSet::new();
    match db.list_repo_watch_levels(repo_id).await {
        Ok(rows) => candidates.extend(rows.into_iter().map(|(uid, _)| uid)),
        Err(e) => {
            tracing::warn!(error = %e, repo_id = %repo_id, "access sweep: watch list failed")
        }
    }
    match db.list_notification_recipient_ids_for_repo(repo_id).await {
        Ok(ids) => candidates.extend(ids),
        Err(e) => {
            tracing::warn!(error = %e, repo_id = %repo_id, "access sweep: recipients failed")
        }
    }
    for user_id in candidates {
        prune_if_repo_read_lost(db, &user_id, repo_id).await;
    }
}

/// One user's read access re-checked against every repo owned by `owner_id` —
/// used when a single membership change (org removal, role demotion,
/// collaborator removal on an org-owned repo) may revoke access broadly.
/// Soft-fails.
pub async fn sweep_user_access_on_owner_repos(db: &Database, user_id: &str, owner_id: &str) {
    match db.list_repositories_by_owner(owner_id).await {
        Ok(repos) => {
            for repo in repos {
                prune_if_repo_read_lost(db, user_id, &repo.id).await;
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, owner_id = %owner_id, "access sweep: repo list failed")
        }
    }
}

/// Re-check every watcher/recipient on every repo owned by `owner_id` — used
/// after org-wide ACL changes such as `member_base_permission` → `none`.
/// Soft-fails.
pub async fn sweep_org_access(db: &Database, org_id: &str) {
    match db.list_repositories_by_owner(org_id).await {
        Ok(repos) => {
            for repo in repos {
                sweep_repo_access(db, &repo.id).await;
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, org_id = %org_id, "access sweep: repo list failed")
        }
    }
}

/// Read-time access re-check (DEBT-06 follow-up): every repository the user
/// still holds notifications for is re-validated; rows for repos they can no
/// longer read are deleted together with the watch row.
pub async fn prune_stale_notifications(db: &Database, user_id: &str) {
    match db.list_notification_repo_ids_for_recipient(user_id).await {
        Ok(repo_ids) => {
            for repo_id in repo_ids {
                prune_if_repo_read_lost(db, user_id, &repo_id).await;
            }
        }
        Err(e) => {
            tracing::warn!(error = %e, user_id = %user_id, "stale notification sweep failed")
        }
    }
}

/// Extract `@username` handles from plain text.
pub fn extract_mention_usernames(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'@' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() {
                let c = bytes[end] as char;
                if c.is_ascii_alphanumeric() || c == '-' {
                    end += 1;
                    if end - start > 39 {
                        break;
                    }
                } else {
                    break;
                }
            }
            if end > start && end - start <= 39 {
                let prev_ok = i == 0
                    || matches!(
                        bytes[i - 1] as char,
                        ' ' | '\t' | '\n' | '\r' | '(' | '[' | '{' | ',' | ':' | ';'
                    );
                if prev_ok {
                    if let Ok(name) = std::str::from_utf8(&bytes[start..end]) {
                        let lower = name.to_ascii_lowercase();
                        if seen.insert(lower.clone()) {
                            out.push(lower);
                        }
                    }
                }
            }
            i = end.max(i + 1);
            continue;
        }
        i += 1;
    }
    out
}

/// Resolve `@username` mentions to user ids (unknown handles ignored).
pub async fn resolve_mention_user_ids(db: &Database, body: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for username in extract_mention_usernames(body) {
        match db.find_user_by_username(&username).await {
            Ok(Some(u)) => ids.push(u.id),
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(error = %e, username = %username, "mention resolve failed");
            }
        }
    }
    ids
}

/// Issue participants: author + assignees + prior commenters (D-03).
pub async fn issue_participant_ids(db: &Database, issue_id: &str, author_id: &str) -> Vec<String> {
    let mut ids = HashSet::new();
    if !author_id.is_empty() {
        ids.insert(author_id.to_string());
    }
    match db.list_issue_assignees(issue_id).await {
        Ok(rows) => {
            for a in rows {
                ids.insert(a.user_id);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_issue_assignees for notify failed"),
    }
    match db.list_issue_comments(issue_id).await {
        Ok(rows) => {
            for c in rows {
                ids.insert(c.author_id);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_issue_comments for notify failed"),
    }
    ids.into_iter().collect()
}

pub fn subject_for_issue(issue: &oxidean_db::IssueRow) -> NotifySubject {
    NotifySubject {
        kind: "issue",
        repo_id: issue.repo_id.clone(),
        number: issue.number,
        title: issue.title.clone(),
        subject_ref: None,
    }
}

pub fn subject_for_pull(pull: &oxidean_db::PullRow) -> NotifySubject {
    NotifySubject {
        kind: "pull_request",
        repo_id: pull.repo_id.clone(),
        number: pull.number,
        title: pull.title.clone(),
        subject_ref: None,
    }
}

/// Release subjects carry the tag in `subject_ref` for deep links.
pub fn subject_for_release(repo_id: &str, tag_name: &str, title: &str) -> NotifySubject {
    NotifySubject {
        kind: "release",
        repo_id: repo_id.to_string(),
        number: 0,
        title: if title.is_empty() {
            tag_name.to_string()
        } else {
            title.to_string()
        },
        subject_ref: Some(tag_name.to_string()),
    }
}

/// Workflow-run subjects carry the run id in `subject_ref` for deep links.
pub fn subject_for_workflow_run(run: &oxidean_db::ActionRunRow) -> NotifySubject {
    let title = if run.title.is_empty() {
        run.workflow_name.clone()
    } else {
        run.title.clone()
    };
    NotifySubject {
        kind: "workflow_run",
        repo_id: run.repository_id.clone(),
        number: 0,
        title,
        subject_ref: Some(run.id.clone()),
    }
}

/// PR participants: author + prior commenters + requested reviewers (D-02 / D-03).
pub async fn pull_participant_ids(db: &Database, pull_id: &str, author_id: &str) -> Vec<String> {
    let mut ids = HashSet::new();
    if !author_id.is_empty() {
        ids.insert(author_id.to_string());
    }
    match db.list_pull_comments(pull_id).await {
        Ok(rows) => {
            for c in rows {
                ids.insert(c.author_id);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_pull_comments for notify failed"),
    }
    match db.list_pull_review_request_user_ids(pull_id).await {
        Ok(rows) => {
            for uid in rows {
                ids.insert(uid);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_pull_review_requests for notify failed"),
    }
    match db.list_pull_reviews(pull_id).await {
        Ok(rows) => {
            for r in rows {
                ids.insert(r.author_id);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_pull_reviews for notify failed"),
    }
    ids.into_iter().collect()
}
