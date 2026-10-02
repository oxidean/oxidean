//! Best-effort notification fan-out after domain writes (NOTF-01 / D-03).

use std::collections::HashSet;

use uuid::Uuid;

use crate::rpc::RpcCtx;

/// Subject metadata for a notification row.
#[derive(Debug, Clone)]
pub struct NotifySubject {
    pub kind: &'static str,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
}

/// Insert one notification per recipient, excluding the actor (D-03).
/// Soft-fails: logs errors and does not abort the caller.
pub async fn fanout(
    ctx: &RpcCtx,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
) {
    let mut seen = HashSet::new();
    for recipient_id in recipients {
        if recipient_id.is_empty() || recipient_id == actor_id {
            continue;
        }
        if !seen.insert(recipient_id.clone()) {
            continue;
        }
        let id = Uuid::new_v4().to_string();
        if let Err(e) = ctx
            .db
            .insert_notification(
                &id,
                &recipient_id,
                actor_id,
                reason,
                subject.kind,
                &subject.repo_id,
                subject.number,
                &subject.title,
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
    ctx: &RpcCtx,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
) {
    let base: HashSet<String> = recipients
        .into_iter()
        .filter(|r| !r.is_empty() && r != actor_id)
        .collect();
    match ctx.db.list_repo_watch_levels(&subject.repo_id).await {
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
            fanout(ctx, actor_id, out, reason, subject).await;
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                repo_id = %subject.repo_id,
                "watch-level lookup failed; participant-only fanout"
            );
            fanout(ctx, actor_id, base, reason, subject).await;
        }
    }
}

/// Suppression-only variant for direct-address notifications (mentions,
/// assignments, review requests): the caller-computed recipients pass through
/// unchanged except watchers at `ignore`, which suppresses even @-mentions
/// (GitHub "Ignore" semantics, DEBT-06).
pub async fn fanout_suppress_ignored(
    ctx: &RpcCtx,
    actor_id: &str,
    recipients: impl IntoIterator<Item = String>,
    reason: &str,
    subject: &NotifySubject,
) {
    let list: Vec<String> = recipients.into_iter().collect();
    match ctx.db.list_repo_watch_levels(&subject.repo_id).await {
        Ok(rows) => {
            let ignored: HashSet<&str> = rows
                .iter()
                .filter(|(_, l)| l == "ignore")
                .map(|(uid, _)| uid.as_str())
                .collect();
            fanout(
                ctx,
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
            fanout(ctx, actor_id, list, reason, subject).await;
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
pub async fn resolve_mention_user_ids(ctx: &RpcCtx, body: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for username in extract_mention_usernames(body) {
        match ctx.db.find_user_by_username(&username).await {
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
pub async fn issue_participant_ids(ctx: &RpcCtx, issue_id: &str, author_id: &str) -> Vec<String> {
    let mut ids = HashSet::new();
    if !author_id.is_empty() {
        ids.insert(author_id.to_string());
    }
    match ctx.db.list_issue_assignees(issue_id).await {
        Ok(rows) => {
            for a in rows {
                ids.insert(a.user_id);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_issue_assignees for notify failed"),
    }
    match ctx.db.list_issue_comments(issue_id).await {
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
    }
}

pub fn subject_for_pull(pull: &oxidean_db::PullRow) -> NotifySubject {
    NotifySubject {
        kind: "pull_request",
        repo_id: pull.repo_id.clone(),
        number: pull.number,
        title: pull.title.clone(),
    }
}

/// PR participants: author + prior commenters + requested reviewers (D-02 / D-03).
pub async fn pull_participant_ids(ctx: &RpcCtx, pull_id: &str, author_id: &str) -> Vec<String> {
    let mut ids = HashSet::new();
    if !author_id.is_empty() {
        ids.insert(author_id.to_string());
    }
    match ctx.db.list_pull_comments(pull_id).await {
        Ok(rows) => {
            for c in rows {
                ids.insert(c.author_id);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_pull_comments for notify failed"),
    }
    match ctx.db.list_pull_review_request_user_ids(pull_id).await {
        Ok(rows) => {
            for uid in rows {
                ids.insert(uid);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_pull_review_requests for notify failed"),
    }
    match ctx.db.list_pull_reviews(pull_id).await {
        Ok(rows) => {
            for r in rows {
                ids.insert(r.author_id);
            }
        }
        Err(e) => tracing::warn!(error = %e, "list_pull_reviews for notify failed"),
    }
    ids.into_iter().collect()
}
