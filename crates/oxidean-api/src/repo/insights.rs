//! Repo insights — contributors, weekly commit activity, fork network (GIT-26).
//!
//! The git-side aggregations are bounded by `*_SCAN_MAX_COMMITS` caps and
//! report `truncated` when the walk was clipped, so huge repositories stay
//! cheap. Fork-network rows come from the tri-dialect query in `oxidean-db`.

use chrono::Utc;
use oxidean_core::{
    AppError, RepoCommitActivityWeek, RepoForkNetworkNode, RepoInsightContributor,
    RepoInsightsCommitActivityRequest, RepoInsightsCommitActivityResponse,
    RepoInsightsContributorsRequest, RepoInsightsContributorsResponse,
    RepoInsightsForkNetworkRequest, RepoInsightsForkNetworkResponse,
};

use super::acl::resolve_repo_for_read;
use super::author_resolve::resolve_author_emails;
use crate::git::bare_repo_path;
use crate::rpc::RpcCtx;

/// Max commits walked for the contributors scan (bounded git cost, GIT-26).
const CONTRIBUTOR_SCAN_MAX_COMMITS: u64 = 50_000;
/// Max commits walked for the weekly activity scan.
const ACTIVITY_SCAN_MAX_COMMITS: u64 = 100_000;
/// Window for weekly activity buckets (default matches GitHub's year view).
const ACTIVITY_DEFAULT_WEEKS: i64 = 52;
const ACTIVITY_MAX_WEEKS: i64 = 104;
/// Max fork-network members returned by `repo.insights.forkNetwork`.
const FORK_NETWORK_DEFAULT_LIMIT: i64 = 100;
const FORK_NETWORK_MAX_LIMIT: i64 = 500;

const DAY_SECS: i64 = 86_400;
const WEEK_SECS: i64 = 7 * DAY_SECS;

fn db_err(e: String) -> AppError {
    if e == "database not configured" {
        AppError::new(
            "db.not_configured",
            "no database configured for this instance",
        )
    } else {
        tracing::error!(error = %e, "insights db error");
        AppError::new("repo.internal", "repository operation failed")
    }
}

/// Sunday 00:00:00 UTC of the week containing `unix` (GitHub week convention).
fn week_start(unix: i64) -> i64 {
    let day_index = unix.div_euclid(DAY_SECS);
    // 1970-01-01 (day 0) was a Thursday -> weekday (0=Sun) = (day + 4) mod 7.
    let weekday = (day_index + 4).rem_euclid(7);
    (day_index - weekday) * DAY_SECS
}

/// Fill Sunday-anchored week buckets (oldest-first, current week last) from a
/// committer-timestamp walk. Times outside the window are skipped (git
/// `--since` can leak boundary commits); anything newer than the last bucket
/// clamps into the in-progress week. Returns buckets + counted total.
fn bucket_commit_times(
    times: &[i64],
    window_start: i64,
    week_count: usize,
) -> (Vec<RepoCommitActivityWeek>, i64) {
    let mut weeks: Vec<RepoCommitActivityWeek> = (0..week_count)
        .map(|i| RepoCommitActivityWeek {
            week: window_start + i as i64 * WEEK_SECS,
            days: [0; 7],
            total: 0,
        })
        .collect();
    let mut total = 0i64;
    for &t in times {
        if t < window_start || weeks.is_empty() {
            continue;
        }
        let idx = ((t - window_start) / WEEK_SECS).min(week_count as i64 - 1) as usize;
        let day = ((t - weeks[idx].week) / DAY_SECS).clamp(0, 6) as usize;
        weeks[idx].days[day] += 1;
        weeks[idx].total += 1;
        total += 1;
    }
    (weeks, total)
}

/// `repo.insights.contributors` — top committers on the default branch with
/// account mapping (exact email / forge noreply via [`resolve_author_emails`]).
pub async fn contributors(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoInsightsContributorsResponse, AppError> {
    let req: RepoInsightsContributorsRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.insights.contributors input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let limit = req.limit.unwrap_or(30).clamp(1, 100) as u32;
    let scan = ctx
        .git
        .contributor_scan(
            &path,
            &accessible.row.default_branch,
            limit,
            CONTRIBUTOR_SCAN_MAX_COMMITS,
        )
        .await
        .map_err(super::map_git_err)?;
    let resolved =
        resolve_author_emails(&ctx.db, scan.contributors.iter().map(|c| c.email.as_str())).await;
    let contributors = scan
        .contributors
        .into_iter()
        .map(|c| {
            let hit = resolved.get(&c.email);
            RepoInsightContributor {
                name: c.name,
                email: c.email,
                commit_count: c.commit_count,
                username: hit.and_then(|r| r.username.clone()),
                avatar_url: hit.and_then(|r| r.avatar_url.clone()),
                first_commit_sha: c.first_commit_sha,
                first_commit_unix: c.first_commit_unix,
                last_commit_sha: c.last_commit_sha,
                last_commit_unix: c.last_commit_unix,
            }
        })
        .collect();
    Ok(RepoInsightsContributorsResponse {
        contributors,
        scanned_commits: scan.scanned_commits,
        truncated: scan.truncated,
    })
}

/// `repo.insights.commitActivity` — GitHub `/stats/commit_activity`-shaped
/// weekly buckets over the default branch (bounded committer-date walk).
pub async fn commit_activity(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoInsightsCommitActivityResponse, AppError> {
    let req: RepoInsightsCommitActivityRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.insights.commitActivity input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let path = bare_repo_path(
        &ctx.repos_dir,
        &accessible.owner_username,
        &accessible.row.name,
    )?;
    let week_count = req
        .weeks
        .unwrap_or(ACTIVITY_DEFAULT_WEEKS)
        .clamp(1, ACTIVITY_MAX_WEEKS) as usize;

    // Window ends with the current in-progress week (Sunday 00:00 UTC anchor).
    let current_week_start = week_start(Utc::now().timestamp());
    let window_start = current_week_start - (week_count as i64 - 1) * WEEK_SECS;

    let res = ctx
        .git
        .commit_times(
            &path,
            &accessible.row.default_branch,
            Some(window_start),
            ACTIVITY_SCAN_MAX_COMMITS,
        )
        .await
        .map_err(super::map_git_err)?;

    let (weeks, total) = bucket_commit_times(&res.times, window_start, week_count);
    Ok(RepoInsightsCommitActivityResponse {
        weeks,
        total,
        scanned_commits: res.times.len() as u64,
        truncated: res.truncated,
    })
}

/// `repo.insights.forkNetwork` — fork-network member list (root + public
/// forks + the queried repo itself), oldest-first for one-pass tree building.
pub async fn fork_network(
    ctx: &RpcCtx,
    input: serde_json::Value,
) -> Result<RepoInsightsForkNetworkResponse, AppError> {
    let req: RepoInsightsForkNetworkRequest = serde_json::from_value(input).map_err(|e| {
        AppError::new(
            "rpc.bad_input",
            format!("invalid repo.insights.forkNetwork input: {e}"),
        )
    })?;
    let accessible = resolve_repo_for_read(ctx, &req.owner, &req.name).await?;
    let network_id = ctx
        .db
        .get_repo_fork_network_id(&accessible.row.id)
        .await
        .map_err(db_err)?
        .unwrap_or_else(|| accessible.row.id.clone());
    let limit = req
        .limit
        .unwrap_or(FORK_NETWORK_DEFAULT_LIMIT)
        .clamp(1, FORK_NETWORK_MAX_LIMIT);
    let total = ctx
        .db
        .count_fork_network_members(&network_id, &accessible.row.id)
        .await
        .map_err(db_err)?;
    let rows = ctx
        .db
        .list_fork_network(&network_id, &accessible.row.id, limit)
        .await
        .map_err(db_err)?;
    let truncated = total > rows.len() as i64;
    let nodes = rows
        .into_iter()
        .map(|r| {
            let owner_avatar_url = match (&r.owner_user_id, r.has_owner_avatar) {
                (Some(uid), true) => Some(format!("/uploads/avatars/{uid}.webp")),
                _ => None,
            };
            RepoForkNetworkNode {
                is_root: r.id == network_id,
                is_current: r.id == accessible.row.id,
                id: r.id,
                owner: r.owner_username,
                name: r.name,
                parent_owner: r.parent_owner,
                parent_name: r.parent_name,
                star_count: r.star_count,
                fork_count: r.fork_count,
                created_at: r.created_at,
                updated_at: r.updated_at,
                owner_avatar_url,
            }
        })
        .collect();
    Ok(RepoInsightsForkNetworkResponse {
        nodes,
        total,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_start_anchors_on_sunday() {
        // 2026-09-27 (Sun) 00:00 UTC = 1790467200.
        let sunday = 1_790_467_200i64;
        assert_eq!(week_start(sunday), sunday);
        assert_eq!(week_start(sunday + 3 * DAY_SECS), sunday); // Wednesday
        assert_eq!(week_start(sunday + 6 * DAY_SECS + 86_399), sunday); // Sat 23:59
        assert_eq!(week_start(sunday + 7 * DAY_SECS), sunday + WEEK_SECS); // next Sun
        assert_eq!(week_start(sunday - 1), sunday - WEEK_SECS); // Sat prior
    }

    #[test]
    fn bucket_commit_times_counts_days_and_total() {
        let sunday = 1_790_467_200i64; // 2026-09-27 Sun 00:00 UTC
        let window_start = sunday - WEEK_SECS; // two-week window
        let times = [
            sunday + 60,               // current week, Sunday
            sunday + 2 * DAY_SECS + 5, // current week, Tuesday
            sunday - 10,               // prior week, Saturday
            window_start - 5,          // before window — skipped
            sunday + 8 * DAY_SECS,     // beyond window — clamps into last week
        ];
        let (weeks, total) = bucket_commit_times(&times, window_start, 2);
        assert_eq!(weeks.len(), 2);
        assert_eq!(weeks[0].week, window_start);
        assert_eq!(weeks[0].days[6], 1); // Saturday of week 0
        assert_eq!(weeks[0].total, 1);
        assert_eq!(weeks[1].days[0], 1); // Sunday
        assert_eq!(weeks[1].days[2], 1); // Tuesday
        assert_eq!(weeks[1].days[6], 1); // beyond-window commit clamps to Sat
        assert_eq!(weeks[1].total, 3);
        assert_eq!(total, 4);
    }

    #[test]
    fn bucket_commit_times_empty() {
        let (weeks, total) = bucket_commit_times(&[], 1_790_467_200, 52);
        assert_eq!(weeks.len(), 52);
        assert_eq!(total, 0);
        assert!(weeks.iter().all(|w| w.total == 0 && w.days == [0; 7]));
    }
}
