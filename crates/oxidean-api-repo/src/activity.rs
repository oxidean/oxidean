//! `repo.activity.list` handler (issue #162). Recording machinery lives in
//! `oxidean_api_core::repo::activity`.

use chrono::{Duration, Utc};
use oxidean_api_core::repo::resolve_repo_for_read;
use oxidean_api_core::repo::{short_ref, VALID_PUSH_TYPES};
use oxidean_api_core::rpc::RpcCtx;
use oxidean_core::{
    AppError, RepoActivityActor, RepoActivityItem, RepoActivityListRequest,
    RepoActivityListResponse,
};

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
        ref_short: row.ref_name.as_deref().map(short_ref),
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
        // `vcs.*` rows land here via the shared activity recorder; their
        // kinds are valid filter values too (`VCS_ACTIVITY_KINDS` — the
        // extension point u1 seeded for u7).
        if !VALID_PUSH_TYPES.contains(&pt.as_str())
            && !oxidean_core::VCS_ACTIVITY_KINDS.contains(&pt.as_str())
        {
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
