//! Per-user contribution events across the activity tables — feeds
//! `user.getContributions` (COL-12). One UNION ALL over authored commits
//! (`authored_commits`), pull requests opened, issues opened, reviews
//! submitted, and repositories created.

use sqlx::Row;

use crate::pool::DbPool;

/// A normalized contribution event. `count` is the commits_count for
/// `commits` rows and `1` for everything else.
#[derive(Debug, Clone)]
pub struct ContributionEventRow {
    /// `YYYY-MM-DD`.
    pub day: String,
    /// `commits` | `pull_request` | `issue` | `review` | `repository`.
    pub kind: String,
    /// `owner/name`.
    pub repo_slug: String,
    /// PR/issue number for itemized kinds.
    pub ref_number: Option<i64>,
    /// PR/issue title for itemized kinds.
    pub title: Option<String>,
    pub count: i64,
}

macro_rules! map_event {
    ($row:expr) => {{
        let row = $row;
        let ref_number: Option<i64> = row
            .try_get::<Option<i64>, _>("ref_number")
            .or_else(|_| {
                row.try_get::<Option<i32>, _>("ref_number")
                    .map(|o| o.map(i64::from))
            })
            .unwrap_or(None);
        let count: i64 = row
            .try_get::<i64, _>("count")
            .or_else(|_| row.try_get::<i32, _>("count").map(i64::from))
            .map_err(|e| format!("contribution row: {e}"))?;
        ContributionEventRow {
            day: row
                .try_get("day")
                .map_err(|e| format!("contribution row: {e}"))?,
            kind: row
                .try_get("kind")
                .map_err(|e| format!("contribution row: {e}"))?,
            repo_slug: row
                .try_get("repo_slug")
                .map_err(|e| format!("contribution row: {e}"))?,
            ref_number,
            title: row.try_get::<Option<String>, _>("title").unwrap_or(None),
            count,
        }
    }};
}

// Owner-slug join arm, shared by every table so slugs render `owner/name`
// for both user- and org-owned repos.
const OWNER_JOINS: &str = "LEFT JOIN users ru ON r.owner_type = 'user' AND ru.id = r.owner_id
LEFT JOIN organizations ro ON r.owner_type = 'org' AND ro.id = r.owner_id";

fn pg_day(col: &str) -> String {
    format!("to_char({col} AT TIME ZONE 'UTC', 'YYYY-MM-DD')")
}
fn mysql_day(col: &str) -> String {
    format!("DATE_FORMAT({col}, '%Y-%m-%d')")
}
fn sqlite_day(col: &str) -> String {
    format!("strftime('%Y-%m-%d', {col})")
}

/// Build the UNION ALL body for one dialect. `{vis}` expands to the
/// public-visibility clause per arm (each arm's repo alias is `r`).
fn build_query(day: fn(&str) -> String, concat: fn(&str, &str) -> String) -> String {
    let slug = concat("COALESCE(ru.username, ro.slug)", "r.name");
    let commits = format!(
        "SELECT c.authored_day AS day, 'commits' AS kind, {slug} AS repo_slug,\n\
         NULL AS ref_number, NULL AS title, COUNT(*) AS count\n\
         FROM authored_commits c\n\
         INNER JOIN repositories r ON r.id = c.repository_id\n{OWNER_JOINS}\n\
         WHERE c.user_id = {{p1}}\n\
         AND c.authored_day >= {{dp2}} AND c.authored_day < {{dp3}}{{vis}}\n\
         GROUP BY c.authored_day, repo_slug",
    );
    let pulls = format!(
        "SELECT {day} AS day, 'pull_request' AS kind, {slug} AS repo_slug,\n\
         p.number AS ref_number, p.title AS title, 1 AS count\n\
         FROM pull_requests p\n\
         INNER JOIN repositories r ON r.id = p.repo_id\n{OWNER_JOINS}\n\
         WHERE p.author_id = {{p1}}\n\
         AND p.created_at >= {{p2}} AND p.created_at < {{p3}}{{vis}}",
        day = day("p.created_at"),
    );
    let issues = format!(
        "SELECT {day} AS day, 'issue' AS kind, {slug} AS repo_slug,\n\
         i.number AS ref_number, i.title AS title, 1 AS count\n\
         FROM issues i\n\
         INNER JOIN repositories r ON r.id = i.repo_id\n{OWNER_JOINS}\n\
         WHERE i.author_id = {{p1}}\n\
         AND i.created_at >= {{p2}} AND i.created_at < {{p3}}{{vis}}",
        day = day("i.created_at"),
    );
    let reviews = format!(
        "SELECT {day} AS day, 'review' AS kind, {slug} AS repo_slug,\n\
         p.number AS ref_number, p.title AS title, 1 AS count\n\
         FROM pull_reviews rv\n\
         INNER JOIN pull_requests p ON p.id = rv.pull_id\n\
         INNER JOIN repositories r ON r.id = p.repo_id\n{OWNER_JOINS}\n\
         WHERE rv.author_id = {{p1}}\n\
         AND rv.submitted_at >= {{p2}} AND rv.submitted_at < {{p3}}{{vis}}",
        day = day("rv.submitted_at"),
    );
    let repos = format!(
        "SELECT {day} AS day, 'repository' AS kind, {slug} AS repo_slug,\n\
         NULL AS ref_number, NULL AS title, 1 AS count\n\
         FROM repositories r\n{OWNER_JOINS}\n\
         WHERE r.owner_type = 'user' AND r.owner_id = {{p1}}\n\
         AND r.created_at >= {{p2}} AND r.created_at < {{p3}}{{vis}}",
        day = day("r.created_at"),
    );
    format!("{commits}\nUNION ALL\n{pulls}\nUNION ALL\n{issues}\nUNION ALL\n{reviews}\nUNION ALL\n{repos}")
}

fn pg_concat(a: &str, b: &str) -> String {
    format!("{a} || '/' || {b}")
}
fn mysql_concat(a: &str, b: &str) -> String {
    format!("CONCAT({a}, '/', {b})")
}
fn sqlite_concat(a: &str, b: &str) -> String {
    format!("{a} || '/' || {b}")
}

/// Normalized contribution events for `user.getContributions`: one row per
/// event (commits rows carry `commits_count` in `count`), `from` inclusive
/// and `to_exclusive` exclusive `YYYY-MM-DD` bounds.
pub async fn list_user_contribution_events(
    pool: &DbPool,
    user_id: &str,
    from: &str,
    to_exclusive: &str,
    public_only: bool,
) -> Result<Vec<ContributionEventRow>, String> {
    let vis = if public_only {
        "\n         AND r.visibility = 'public'"
    } else {
        ""
    };
    match pool {
        DbPool::Postgres(p) => {
            let body = build_query(pg_day, pg_concat)
                .replace("{p1}", "$1")
                .replace("{p2}", "$2::timestamptz")
                .replace("{p3}", "$3::timestamptz")
                .replace("{dp2}", "$2")
                .replace("{dp3}", "$3")
                .replace("{vis}", vis);
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*body))
                .bind(user_id)
                .bind(from)
                .bind(to_exclusive)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list user contribution events failed: {e}"))?;
            rows.iter().map(|r| Ok(map_event!(r))).collect()
        }
        DbPool::MySql(p) => {
            let body = build_query(mysql_day, mysql_concat)
                .replace("{p1}", "?")
                .replace("{p2}", "?")
                .replace("{p3}", "?")
                .replace("{dp2}", "?")
                .replace("{dp3}", "?")
                .replace("{vis}", vis);
            let mut q = sqlx::query(sqlx::AssertSqlSafe(&*body));
            for _ in 0..5 {
                q = q.bind(user_id).bind(from).bind(to_exclusive);
            }
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list user contribution events failed: {e}"))?;
            rows.iter().map(|r| Ok(map_event!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let body = build_query(sqlite_day, sqlite_concat)
                .replace("{p1}", "?1")
                .replace("{p2}", "?2")
                .replace("{p3}", "?3")
                .replace("{dp2}", "?2")
                .replace("{dp3}", "?3")
                .replace("{vis}", vis);
            let rows = sqlx::query(sqlx::AssertSqlSafe(&*body))
                .bind(user_id)
                .bind(from)
                .bind(to_exclusive)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list user contribution events failed: {e}"))?;
            rows.iter().map(|r| Ok(map_event!(r))).collect()
        }
    }
}
