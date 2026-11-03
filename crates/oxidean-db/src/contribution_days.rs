//! `user_contribution_days` — materialized contribution rollup (migration
//! 0095). One row per `(user_id, repository_id, day, kind)`; bumped on write
//! inside the facade's insert paths, rebuilt lazily per user from the source
//! tables, and retagged when a repo's visibility flips.
//!
//! Kinds mirror `ContributionEventRow.kind`: `commits`, `pull_request`,
//! `issue`, `review`, `repository`. `is_private` snapshots repo visibility at
//! write time so public viewers never count private-repo activity; the repo
//! FK cascades the rows away on hard delete.

use sqlx::Row;

use crate::pool::DbPool;

/// First 10 chars of an ISO timestamp (`YYYY-MM-DD…`) — the rollup day.
pub fn day_of(ts: &str) -> &str {
    ts.get(..10).unwrap_or(ts)
}

/// One (day, kind, count) bucket for `user.getContributions`.
#[derive(Debug, Clone)]
pub struct ContributionDayCount {
    pub day: String,
    pub kind: String,
    pub count: i64,
}

macro_rules! map_day_count {
    ($row:expr) => {{
        let row = $row;
        let count: i64 = row
            .try_get::<i64, _>("c")
            .or_else(|_| row.try_get::<i32, _>("c").map(i64::from))
            .map_err(|e| format!("contribution day row: {e}"))?;
        ContributionDayCount {
            day: row.try_get("day").map_err(|e| format!("day row: {e}"))?,
            kind: row.try_get("kind").map_err(|e| format!("day row: {e}"))?,
            count,
        }
    }};
}

/// Upsert `(user, repo, day, kind)` by `n`, resolving `is_private` from the
/// repo's current visibility in the same statement.
pub async fn bump(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
    day: &str,
    kind: &str,
    n: i64,
) -> Result<(), String> {
    if n == 0 || day.len() != 10 {
        return Ok(());
    }
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO user_contribution_days
                   (user_id, repository_id, day, kind, is_private, day_count)
                 SELECT $1, $2, $3, $4, (r.visibility <> 'public'), $5
                 FROM repositories r WHERE r.id = $2
                 ON CONFLICT (user_id, repository_id, day, kind)
                 DO UPDATE SET day_count = user_contribution_days.day_count + EXCLUDED.day_count",
            )
            .bind(user_id)
            .bind(repository_id)
            .bind(day)
            .bind(kind)
            .bind(n)
            .execute(p)
            .await
            .map_err(|e| format!("contribution day bump failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO user_contribution_days
                   (user_id, repository_id, day, kind, is_private, day_count)
                 SELECT ?, ?, ?, ?, (r.visibility <> 'public'), ?
                 FROM repositories r WHERE r.id = ?
                 ON DUPLICATE KEY UPDATE day_count = day_count + VALUES(day_count)",
            )
            .bind(user_id)
            .bind(repository_id)
            .bind(day)
            .bind(kind)
            .bind(n)
            .bind(repository_id)
            .execute(p)
            .await
            .map_err(|e| format!("contribution day bump failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO user_contribution_days
                   (user_id, repository_id, day, kind, is_private, day_count)
                 SELECT ?1, ?2, ?3, ?4, (r.visibility <> 'public'), ?5
                 FROM repositories r WHERE r.id = ?2
                 ON CONFLICT (user_id, repository_id, day, kind)
                 DO UPDATE SET day_count = day_count + excluded.day_count",
            )
            .bind(user_id)
            .bind(repository_id)
            .bind(day)
            .bind(kind)
            .bind(n)
            .execute(p)
            .await
            .map_err(|e| format!("contribution day bump failed: {e}"))?;
        }
    }
    Ok(())
}

/// Same upsert for events whose repo is only known via `pull_requests.id`
/// (review submissions).
pub async fn bump_for_pull(
    pool: &DbPool,
    user_id: &str,
    pull_id: &str,
    day: &str,
    kind: &str,
    n: i64,
) -> Result<(), String> {
    if n == 0 || day.len() != 10 {
        return Ok(());
    }
    let repo_id: Option<String> = match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT repo_id FROM pull_requests WHERE id = $1")
                .bind(pull_id)
                .fetch_optional(p)
                .await
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT repo_id FROM pull_requests WHERE id = ?")
                .bind(pull_id)
                .fetch_optional(p)
                .await
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT repo_id FROM pull_requests WHERE id = ?1")
                .bind(pull_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| format!("contribution pull repo lookup failed: {e}"))?;
    let Some(repo_id) = repo_id else {
        return Ok(());
    };
    bump(pool, user_id, &repo_id, day, kind, n).await
}

/// Per-day, per-kind totals for `user.getContributions`. `public_only`
/// excludes `is_private` buckets (non-owner viewers).
pub async fn list_days(
    pool: &DbPool,
    user_id: &str,
    from: &str,
    to_exclusive: &str,
    public_only: bool,
) -> Result<Vec<ContributionDayCount>, String> {
    let vis = if public_only {
        " AND NOT is_private"
    } else {
        ""
    };
    let sql = format!(
        "SELECT day, kind, SUM(day_count) AS c FROM user_contribution_days\n\
         WHERE user_id = {{p1}} AND day >= {{p2}} AND day < {{p3}}{vis}\n\
         GROUP BY day, kind"
    );
    match pool {
        DbPool::Postgres(p) => {
            let q = sql
                .replace("{p1}", "$1")
                .replace("{p2}", "$2")
                .replace("{p3}", "$3");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(user_id)
                .bind(from)
                .bind(to_exclusive)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list contribution days failed: {e}"))?;
            rows.iter().map(|r| Ok(map_day_count!(r))).collect()
        }
        DbPool::MySql(p) => {
            let q = sql
                .replace("{p1}", "?")
                .replace("{p2}", "?")
                .replace("{p3}", "?");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(user_id)
                .bind(from)
                .bind(to_exclusive)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list contribution days failed: {e}"))?;
            rows.iter().map(|r| Ok(map_day_count!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let q = sql
                .replace("{p1}", "?1")
                .replace("{p2}", "?2")
                .replace("{p3}", "?3");
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*q))
                .bind(user_id)
                .bind(from)
                .bind(to_exclusive)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list contribution days failed: {e}"))?;
            rows.iter().map(|r| Ok(map_day_count!(r))).collect()
        }
    }
}

/// Retag every rollup row for `repo_id` when the repo's visibility flips
/// (`repositories.update_visibility` facade hook).
pub async fn retag_repo(
    pool: &DbPool,
    repository_id: &str,
    is_private: bool,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE user_contribution_days SET is_private = $2 WHERE repository_id = $1",
            )
            .bind(repository_id)
            .bind(is_private)
            .execute(p)
            .await
            .map_err(|e| format!("contribution days retag failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE user_contribution_days SET is_private = ? WHERE repository_id = ?")
                .bind(is_private as i64)
                .bind(repository_id)
                .execute(p)
                .await
                .map_err(|e| format!("contribution days retag failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE user_contribution_days SET is_private = ?2 WHERE repository_id = ?1",
            )
            .bind(repository_id)
            .bind(is_private as i64)
            .execute(p)
            .await
            .map_err(|e| format!("contribution days retag failed: {e}"))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-user rebuild — grouped INSERT-SELECTs over the five source tables.
// `authored_day` is already a TEXT day on authored_commits; the other sources
// derive it from their timestamp columns per dialect.
// ---------------------------------------------------------------------------

fn rebuild_arms(day: fn(&str) -> String, p_user: &str) -> Vec<String> {
    let priv_expr = "(r.visibility <> 'public')";
    vec![
        format!(
            "SELECT c.user_id, c.repository_id, c.authored_day, 'commits', {priv_expr}, COUNT(*)\n\
             FROM authored_commits c JOIN repositories r ON r.id = c.repository_id\n\
             WHERE c.user_id = {p_user}\n\
             GROUP BY c.user_id, c.repository_id, c.authored_day, r.visibility"
        ),
        format!(
            "SELECT p.author_id, p.repo_id, {d} AS day, 'pull_request', {priv_expr}, COUNT(*)\n\
             FROM pull_requests p JOIN repositories r ON r.id = p.repo_id\n\
             WHERE p.author_id = {p_user}\n\
             GROUP BY p.author_id, p.repo_id, day, r.visibility",
            d = day("p.created_at"),
        ),
        format!(
            "SELECT i.author_id, i.repo_id, {d} AS day, 'issue', {priv_expr}, COUNT(*)\n\
             FROM issues i JOIN repositories r ON r.id = i.repo_id\n\
             WHERE i.author_id = {p_user}\n\
             GROUP BY i.author_id, i.repo_id, day, r.visibility",
            d = day("i.created_at"),
        ),
        format!(
            "SELECT rv.author_id, p.repo_id, {d} AS day, 'review', {priv_expr}, COUNT(*)\n\
             FROM pull_reviews rv\n\
             JOIN pull_requests p ON p.id = rv.pull_id\n\
             JOIN repositories r ON r.id = p.repo_id\n\
             WHERE rv.author_id = {p_user}\n\
             GROUP BY rv.author_id, p.repo_id, day, r.visibility",
            d = day("rv.submitted_at"),
        ),
        format!(
            "SELECT r.owner_id, r.id, {d} AS day, 'repository', {priv_expr}, COUNT(*)\n\
             FROM repositories r\n\
             WHERE r.owner_type = 'user' AND r.owner_id = {p_user}\n\
             GROUP BY r.owner_id, r.id, day, r.visibility",
            d = day("r.created_at"),
        ),
    ]
}

/// Delete every rollup row for `user_id` — rebuild/testing helper.
pub async fn clear_user(pool: &DbPool, user_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM user_contribution_days WHERE user_id = $1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("contribution days clear failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM user_contribution_days WHERE user_id = ?")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("contribution days clear failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM user_contribution_days WHERE user_id = ?1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("contribution days clear failed: {e}"))?;
        }
    }
    Ok(())
}

/// Replace all rollup rows for `user_id` with a fresh grouped recompute from
/// the five source tables. Bounded per user; run once per account under the
/// `user:<id>` backfill claim, then kept fresh by write-time bumps.
pub async fn rebuild_user(pool: &DbPool, user_id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM user_contribution_days WHERE user_id = $1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("contribution rebuild delete failed: {e}"))?;
            for arm in rebuild_arms(
                |c| format!("to_char({c} AT TIME ZONE 'UTC', 'YYYY-MM-DD')"),
                "$1",
            ) {
                let q = format!(
                    "INSERT INTO user_contribution_days\n\
                       (user_id, repository_id, day, kind, is_private, day_count)\n{arm}\n\
                     ON CONFLICT (user_id, repository_id, day, kind)\n\
                     DO UPDATE SET day_count = EXCLUDED.day_count, is_private = EXCLUDED.is_private"
                );
                sqlx::query(sqlx::AssertSqlSafe(&*q))
                    .bind(user_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("contribution rebuild insert failed: {e}"))?;
            }
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM user_contribution_days WHERE user_id = ?")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("contribution rebuild delete failed: {e}"))?;
            for arm in rebuild_arms(|c| format!("DATE_FORMAT({c}, '%Y-%m-%d')"), "?") {
                let q = format!(
                    "INSERT INTO user_contribution_days\n\
                       (user_id, repository_id, day, kind, is_private, day_count)\n{arm}\n\
                     ON DUPLICATE KEY UPDATE day_count = VALUES(day_count), is_private = VALUES(is_private)"
                );
                sqlx::query(sqlx::AssertSqlSafe(&*q))
                    .bind(user_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("contribution rebuild insert failed: {e}"))?;
            }
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM user_contribution_days WHERE user_id = ?1")
                .bind(user_id)
                .execute(p)
                .await
                .map_err(|e| format!("contribution rebuild delete failed: {e}"))?;
            for arm in rebuild_arms(|c| format!("strftime('%Y-%m-%d', {c})"), "?1") {
                let q = format!(
                    "INSERT INTO user_contribution_days\n\
                       (user_id, repository_id, day, kind, is_private, day_count)\n{arm}\n\
                     ON CONFLICT (user_id, repository_id, day, kind)\n\
                     DO UPDATE SET day_count = excluded.day_count, is_private = excluded.is_private"
                );
                sqlx::query(sqlx::AssertSqlSafe(&*q))
                    .bind(user_id)
                    .execute(p)
                    .await
                    .map_err(|e| format!("contribution rebuild insert failed: {e}"))?;
            }
        }
    }
    Ok(())
}
