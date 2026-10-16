//! Repository activity feed — durable push / branch / merge events + `repo.activity.list`.

use std::path::Path;
use std::sync::Arc;

use chrono::{Duration, Utc};
use oxidean_core::{
    AppError, RepoActivityActor, RepoActivityItem, RepoActivityListRequest,
    RepoActivityListResponse,
};
use oxidean_db::Database;
use oxidean_git::GitBackend;
use uuid::Uuid;

use super::acl::resolve_repo_for_read;
use crate::rpc::RpcCtx;

const ZERO_OID: &str = "0000000000000000000000000000000000000000";

const VALID_PUSH_TYPES: &[&str] = &[
    "push",
    "force_push",
    "pr_merge",
    "branch_creation",
    "branch_deletion",
    "branch_rename",
];

fn is_zero_oid(oid: &str) -> bool {
    !oid.is_empty() && oid.chars().all(|c| c == '0')
}

fn short_ref(ref_name: &str) -> String {
    ref_name
        .strip_prefix("refs/heads/")
        .or_else(|| ref_name.strip_prefix("refs/tags/"))
        .unwrap_or(ref_name)
        .to_string()
}

fn classify_push_type(before: &str, after: &str, forced: bool) -> &'static str {
    if is_zero_oid(after) {
        "branch_deletion"
    } else if is_zero_oid(before) {
        "branch_creation"
    } else if forced {
        "force_push"
    } else {
        "push"
    }
}

/// Whether `maybe_ancestor` is an ancestor of `tip`.
///
/// - `Some(true)` / `Some(false)` — definitive git result (exit 0 / 1)
/// - `None` — cannot tell (spawn failure, missing objects, bad path); callers must
///   **not** treat this as a force-push (avoids false `force_push` labels).
async fn is_ancestor(bare: &Path, maybe_ancestor: &str, tip: &str) -> Option<bool> {
    if maybe_ancestor.is_empty()
        || tip.is_empty()
        || maybe_ancestor == tip
        || is_zero_oid(maybe_ancestor)
        || is_zero_oid(tip)
    {
        return Some(true);
    }
    let bare_str = bare.to_str()?;
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
        .ok()?;
    match out.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

async fn enrich_commits(
    git: &dyn GitBackend,
    bare: &Path,
    before: &str,
    after: &str,
) -> (i64, Option<String>) {
    if is_zero_oid(after) {
        return (0, None);
    }
    let message = match git.show_commit(bare, after, None, None).await {
        Ok(detail) => {
            let subject = detail.subject.trim();
            if subject.is_empty() {
                None
            } else {
                Some(subject.to_string())
            }
        }
        Err(_) => None,
    };
    let count = if is_zero_oid(before) {
        1
    } else {
        let range = format!("{before}..{after}");
        git.rev_list_count(bare, &range).await.unwrap_or(0) as i64
    };
    (count.max(0), message)
}

/// Best-effort: write one activity row per ref update after a successful receive-pack.
pub async fn record_ref_updates(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    updates: &[(String, String, String)],
    git: Option<Arc<dyn GitBackend>>,
    bare: Option<&Path>,
) {
    if updates.is_empty() || actor_id.is_empty() {
        return;
    }
    for (before, after, ref_name) in updates {
        let forced = if let (_, Some(path)) = (git.as_ref(), bare) {
            !is_zero_oid(before)
                && !is_zero_oid(after)
                && matches!(is_ancestor(path, before, after).await, Some(false))
        } else {
            false
        };
        let push_type = classify_push_type(before, after, forced);
        let (commits_count, commit_message) = if let (Some(g), Some(path)) = (git.as_ref(), bare) {
            enrich_commits(g.as_ref(), path, before, after).await
        } else {
            (0, None)
        };
        let id = Uuid::new_v4().to_string();
        if let Err(e) = db
            .insert_repo_activity(
                &id,
                repository_id,
                actor_id,
                push_type,
                ref_name,
                before,
                after,
                commits_count,
                commit_message.as_deref(),
                None,
            )
            .await
        {
            tracing::warn!(
                error = %e,
                repository_id,
                ref_name,
                "repository activity insert failed"
            );
        }
    }
}

/// Record a UI/API branch create (no receive-pack).
pub async fn record_branch_creation(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    branch: &str,
    after_oid: &str,
) {
    let ref_name = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "branch_creation",
            &ref_name,
            ZERO_OID,
            if after_oid.is_empty() {
                ZERO_OID
            } else {
                after_oid
            },
            0,
            None,
            None,
        )
        .await
    {
        tracing::warn!(error = %e, "branch_creation activity insert failed");
    }
}

/// Record a UI/API branch delete (no receive-pack).
pub async fn record_branch_deletion(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    branch: &str,
    before_oid: &str,
) {
    let ref_name = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "branch_deletion",
            &ref_name,
            if before_oid.is_empty() {
                ZERO_OID
            } else {
                before_oid
            },
            ZERO_OID,
            0,
            None,
            None,
        )
        .await
    {
        tracing::warn!(error = %e, "branch_deletion activity insert failed");
    }
}

/// Record a UI/API branch rename (no receive-pack).
pub async fn record_branch_rename(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    from: &str,
    to: &str,
    tip_oid: &str,
) {
    let ref_name = if to.starts_with("refs/") {
        to.to_string()
    } else {
        format!("refs/heads/{to}")
    };
    let from_short = short_ref(from);
    let after = if tip_oid.is_empty() {
        ZERO_OID
    } else {
        tip_oid
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "branch_rename",
            &ref_name,
            after,
            after,
            0,
            Some(from_short.as_str()),
            None,
        )
        .await
    {
        tracing::warn!(error = %e, "branch_rename activity insert failed");
    }
}

/// Record a successful pull-request merge.
pub async fn record_pr_merge(
    db: &Database,
    repository_id: &str,
    actor_id: &str,
    base_ref: &str,
    merge_sha: &str,
    pr_number: i64,
    commit_message: Option<&str>,
) {
    let ref_name = if base_ref.starts_with("refs/") {
        base_ref.to_string()
    } else {
        format!("refs/heads/{base_ref}")
    };
    let id = Uuid::new_v4().to_string();
    if let Err(e) = db
        .insert_repo_activity(
            &id,
            repository_id,
            actor_id,
            "pr_merge",
            &ref_name,
            ZERO_OID,
            merge_sha,
            1,
            commit_message,
            Some(pr_number),
        )
        .await
    {
        tracing::warn!(error = %e, "pr_merge activity insert failed");
    }
}

fn period_since(period: Option<&str>) -> Option<String> {
    let p = period?.trim().to_ascii_lowercase();
    let days = match p.as_str() {
        "week" | "7d" => 7,
        "month" | "30d" => 30,
        "year" | "365d" => 365,
        "all" | "" => return None,
        _ => return None,
    };
    let since = Utc::now() - Duration::days(days);
    Some(since.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

/// Accept RFC3339 / `YYYY-MM-DDTHH:MM:SSZ` only — reject garbage that would 500 in SQL.
fn parse_since_timestamp(raw: &str) -> Result<String, AppError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(AppError::new(
            "rpc.bad_input",
            "since must be an ISO-8601 UTC timestamp",
        ));
    }
    if chrono::DateTime::parse_from_rfc3339(t).is_ok() {
        return Ok(t.to_string());
    }
    if chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%SZ").is_ok() {
        return Ok(t.to_string());
    }
    if chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S").is_ok() {
        return Ok(t.to_string());
    }
    Err(AppError::new(
        "rpc.bad_input",
        "since must be an ISO-8601 UTC timestamp",
    ))
}

fn row_to_item(row: oxidean_db::RepoActivityRow) -> RepoActivityItem {
    let avatar_url = row
        .actor_avatar_path
        .as_ref()
        .map(|_| format!("/uploads/avatars/{}.webp", row.actor_id));
    let name = if row.actor_display_name.trim().is_empty() {
        row.actor_username.clone()
    } else {
        row.actor_display_name.clone()
    };
    RepoActivityItem {
        id: row.id,
        push_type: row.push_type,
        ref_short: short_ref(&row.ref_name),
        ref_name: row.ref_name,
        before: row.before_oid,
        after: row.after_oid,
        pushed_at: row.created_at,
        commits_count: row.commits_count,
        commit_message: row.commit_message,
        pr_number: row.pr_number,
        pusher: RepoActivityActor {
            path: format!("/{}", row.actor_username),
            login: row.actor_username,
            name,
            avatar_url,
        },
    }
}

/// `repo.activity.list` — chronological push feed behind Read ACL.
pub async fn list(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoActivityListResponse, AppError> {
    let req: RepoActivityListRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.activity.list input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;

    let push_type = req
        .push_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    if let Some(ref pt) = push_type {
        if !VALID_PUSH_TYPES.contains(&pt.as_str()) {
            return Err(AppError::new(
                "rpc.bad_input",
                format!("invalid push_type filter: {pt}"),
            ));
        }
    }

    let since = match req
        .since
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(parse_since_timestamp(raw)?),
        None => period_since(req.period.as_deref()),
    };

    let offset = req.offset.unwrap_or(0).max(0);
    let limit = req.limit.unwrap_or(30).clamp(1, 100);

    let (rows, total) = ctx
        .db
        .list_repo_activity(
            &accessible.row.id,
            push_type.as_deref(),
            since.as_deref(),
            offset,
            limit,
        )
        .await
        .map_err(|e| AppError::new("repo.internal", e))?;

    Ok(RepoActivityListResponse {
        items: rows.into_iter().map(row_to_item).collect(),
        total,
        offset,
        limit,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_create_delete_push_force() {
        assert_eq!(
            classify_push_type(ZERO_OID, "abc", false),
            "branch_creation"
        );
        assert_eq!(
            classify_push_type("abc", ZERO_OID, false),
            "branch_deletion"
        );
        assert_eq!(classify_push_type("aaa", "bbb", false), "push");
        assert_eq!(classify_push_type("aaa", "bbb", true), "force_push");
    }

    #[test]
    fn short_ref_strips_heads_and_tags() {
        assert_eq!(short_ref("refs/heads/main"), "main");
        assert_eq!(short_ref("refs/tags/v1"), "v1");
        assert_eq!(short_ref("main"), "main");
    }

    #[test]
    fn period_since_week_is_some() {
        assert!(period_since(Some("week")).is_some());
        assert!(period_since(Some("all")).is_none());
        assert!(period_since(None).is_none());
    }

    #[test]
    fn parse_since_accepts_iso_rejects_garbage() {
        assert!(parse_since_timestamp("2026-01-02T03:04:05Z").is_ok());
        assert!(parse_since_timestamp("2026-01-02T03:04:05+00:00").is_ok());
        assert!(parse_since_timestamp("not-a-date").is_err());
        assert!(parse_since_timestamp("'; DROP TABLE").is_err());
    }
}
