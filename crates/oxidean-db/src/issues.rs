//! Issues + per-repo `#N` counters (D-ISS-01). Dialect SQL only.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct IssueRow {
    pub id: String,
    pub repo_id: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub author_id: String,
    pub closed_at: Option<String>,
    pub closed_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// Derived `issue_comments` count carried by every `ISSUE_SELECT_*` query.
    pub comment_count: i64,
}

macro_rules! map_issue {
    ($row:expr) => {{
        let row = $row;
        IssueRow {
            id: row.try_get("id").map_err(|e| format!("issue row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("issue row: {e}"))?,
            number: row
                .try_get::<i64, _>("number")
                .map_err(|e| format!("issue row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("issue row: {e}"))?,
            body: row.try_get("body").map_err(|e| format!("issue row: {e}"))?,
            state: row
                .try_get("state")
                .map_err(|e| format!("issue row: {e}"))?,
            author_id: row
                .try_get("author_id")
                .map_err(|e| format!("issue row: {e}"))?,
            closed_at: row
                .try_get("closed_at")
                .map_err(|e| format!("issue row: {e}"))?,
            closed_by: row
                .try_get("closed_by")
                .map_err(|e| format!("issue row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("issue row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("issue row: {e}"))?,
            comment_count: {
                let c: i64 = row
                    .try_get("comment_count")
                    .or_else(|_| row.try_get::<i32, _>("comment_count").map(|v| i64::from(v)))
                    .map_err(|e| format!("issue row: {e}"))?;
                c
            },
        }
    }};
}

const ISSUE_SELECT_PG: &str = "SELECT issues.id, issues.repo_id, issues.number, issues.title, issues.body, issues.state, issues.author_id,
       CASE WHEN issues.closed_at IS NULL THEN NULL
            ELSE to_char(issues.closed_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS closed_at,
       issues.closed_by,
       to_char(issues.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(issues.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at,
       (SELECT COUNT(*)::bigint FROM issue_comments c WHERE c.issue_id = issues.id) AS comment_count
FROM issues";

const ISSUE_SELECT_MYSQL: &str = "SELECT issues.id, issues.repo_id, issues.number, issues.title, issues.body, issues.state, issues.author_id,
       CASE WHEN issues.closed_at IS NULL THEN NULL
            ELSE DATE_FORMAT(issues.closed_at, '%Y-%m-%dT%H:%i:%sZ') END AS closed_at,
       issues.closed_by,
       DATE_FORMAT(issues.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(issues.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at,
       (SELECT COUNT(*) FROM issue_comments c WHERE c.issue_id = issues.id) AS comment_count
FROM issues";

const ISSUE_SELECT_SQLITE: &str = "SELECT issues.id, issues.repo_id, issues.number, issues.title, issues.body, issues.state, issues.author_id,
       CASE WHEN issues.closed_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', issues.closed_at) END AS closed_at,
       issues.closed_by,
       strftime('%Y-%m-%dT%H:%M:%SZ', issues.created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', issues.updated_at) AS updated_at,
       (SELECT COUNT(*) FROM issue_comments c WHERE c.issue_id = issues.id) AS comment_count
FROM issues";

/// Allocate the next per-repo issue number (monotonic; never reclaims).
pub async fn allocate_next_number(pool: &DbPool, repo_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate issue number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issue_counters (repo_id, max_number) VALUES ($1, 0)
                 ON CONFLICT (repo_id) DO NOTHING",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure issue_counters failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "UPDATE issue_counters SET max_number = max_number + 1
                 WHERE repo_id = $1 RETURNING max_number",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("allocate issue number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate issue number tx failed: {e}"))?;
            Ok(number)
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate issue number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issue_counters (repo_id, max_number) VALUES (?, 0)
                 ON DUPLICATE KEY UPDATE repo_id = repo_id",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure issue_counters failed: {e}"))?;
            sqlx::query(
                "UPDATE issue_counters SET max_number = max_number + 1 WHERE repo_id = ?",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allocate issue number failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "SELECT max_number FROM issue_counters WHERE repo_id = ?",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read allocated issue number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate issue number tx failed: {e}"))?;
            Ok(number)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin allocate issue number tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issue_counters (repo_id, max_number) VALUES (?1, 0)
                 ON CONFLICT (repo_id) DO NOTHING",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure issue_counters failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "UPDATE issue_counters SET max_number = max_number + 1
                 WHERE repo_id = ?1 RETURNING max_number",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("allocate issue number failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit allocate issue number tx failed: {e}"))?;
            Ok(number)
        }
    }
}

/// Insert an issue, allocating `#N` in the same transaction as the counter upsert.
pub async fn insert_issue(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    author_id: &str,
    title: &str,
    body: &str,
) -> Result<IssueRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin insert issue tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issue_counters (repo_id, max_number) VALUES ($1, 0)
                 ON CONFLICT (repo_id) DO NOTHING",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure issue_counters failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "UPDATE issue_counters SET max_number = max_number + 1
                 WHERE repo_id = $1 RETURNING max_number",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("allocate issue number failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issues (id, repo_id, number, title, body, state, author_id)
VALUES ($1, $2, $3, $4, $5, 'open', $6)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(author_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert issue failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit insert issue tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin insert issue tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issue_counters (repo_id, max_number) VALUES (?, 0)
                 ON DUPLICATE KEY UPDATE repo_id = repo_id",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure issue_counters failed: {e}"))?;
            sqlx::query(
                "UPDATE issue_counters SET max_number = max_number + 1 WHERE repo_id = ?",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("allocate issue number failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "SELECT max_number FROM issue_counters WHERE repo_id = ?",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("read allocated issue number failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issues (id, repo_id, number, title, body, state, author_id)
VALUES (?, ?, ?, ?, ?, 'open', ?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(author_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert issue failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit insert issue tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin insert issue tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issue_counters (repo_id, max_number) VALUES (?1, 0)
                 ON CONFLICT (repo_id) DO NOTHING",
            )
            .bind(repo_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("ensure issue_counters failed: {e}"))?;
            let number: i64 = sqlx::query_scalar(
                "UPDATE issue_counters SET max_number = max_number + 1
                 WHERE repo_id = ?1 RETURNING max_number",
            )
            .bind(repo_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("allocate issue number failed: {e}"))?;
            sqlx::query(
                "INSERT INTO issues (id, repo_id, number, title, body, state, author_id)
VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(number)
            .bind(title)
            .bind(body)
            .bind(author_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("insert issue failed: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("commit insert issue tx failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert issue failed: row missing after insert".into())
}

pub async fn find_by_id(pool: &DbPool, id: &str) -> Result<Option<IssueRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(&format!("{ISSUE_SELECT_PG} WHERE id = $1"))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!("{ISSUE_SELECT_MYSQL} WHERE id = ?"))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(&format!("{ISSUE_SELECT_SQLITE} WHERE id = ?1"))
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue by id failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue!(&r)),
                None => None,
            })
        }
    }
}

/// Look up an issue by repo id + per-repo number.
pub async fn find_by_repo_number(
    pool: &DbPool,
    repo_id: &str,
    number: i64,
) -> Result<Option<IssueRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(&format!(
                "{ISSUE_SELECT_PG} WHERE repo_id = $1 AND number = $2"
            ))
            .bind(repo_id)
            .bind(number)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find issue by number failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(&format!(
                "{ISSUE_SELECT_MYSQL} WHERE repo_id = ? AND number = ?"
            ))
            .bind(repo_id)
            .bind(number)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find issue by number failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(&format!(
                "{ISSUE_SELECT_SQLITE} WHERE repo_id = ?1 AND number = ?2"
            ))
            .bind(repo_id)
            .bind(number)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find issue by number failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue!(&r)),
                None => None,
            })
        }
    }
}

/// Optional filters for `list_for_repo` (D-ISS-16..18). Ids are resolved by the API layer.
#[derive(Debug, Clone, Default)]
pub struct IssueListFilters<'a> {
    /// `open` | `closed` | `all` (empty → open).
    pub state: &'a str,
    pub author_id: Option<&'a str>,
    pub label_id: Option<&'a str>,
    pub assignee_id: Option<&'a str>,
    /// Simple substring match on title/body (dialect LIKE/ILIKE — not the `key:value` qualifier grammar).
    pub q: Option<&'a str>,
    pub offset: i64,
    pub limit: i64,
}

fn normalize_list_paging(offset: i64, limit: i64) -> (i64, i64) {
    let offset = offset.max(0);
    // Default ~25 (plan ASSUME); cap at 100.
    let limit = if limit <= 0 { 25 } else { limit.min(100) };
    (offset, limit)
}

fn normalize_list_state(state_filter: &str) -> Result<Option<&'static str>, String> {
    match state_filter.trim() {
        "" | "open" => Ok(Some("open")),
        "closed" => Ok(Some("closed")),
        "all" => Ok(None),
        other => Err(format!("invalid issue state filter: {other}")),
    }
}

fn like_pattern(q: &str) -> String {
    format!("%{q}%")
}

/// List issues for a repo with state/author/label/assignee/text filters.
/// Sorted newest-updated first (D-ISS-18). Returns `(rows, total)`.
pub async fn list_for_repo(
    pool: &DbPool,
    repo_id: &str,
    filters: IssueListFilters<'_>,
) -> Result<(Vec<IssueRow>, i64), String> {
    let (offset, limit) = normalize_list_paging(filters.offset, filters.limit);
    let state = normalize_list_state(filters.state)?;
    let author_id = filters.author_id.filter(|s| !s.is_empty());
    let label_id = filters.label_id.filter(|s| !s.is_empty());
    let assignee_id = filters.assignee_id.filter(|s| !s.is_empty());
    let q = filters
        .q
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(like_pattern);

    match pool {
        DbPool::Postgres(p) => {
            let total: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*)::bigint FROM issues
WHERE repo_id = $1
  AND ($2::text IS NULL OR state = $2)
  AND ($3::text IS NULL OR author_id = $3)
  AND ($4::text IS NULL OR EXISTS (
        SELECT 1 FROM issue_labels il WHERE il.issue_id = issues.id AND il.label_id = $4))
  AND ($5::text IS NULL OR EXISTS (
        SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = issues.id AND ia.user_id = $5))
  AND ($6::text IS NULL OR title ILIKE $6 OR body ILIKE $6)"#,
            )
            .bind(repo_id)
            .bind(state)
            .bind(author_id)
            .bind(label_id)
            .bind(assignee_id)
            .bind(q.as_deref())
            .fetch_one(p)
            .await
            .map_err(|e| format!("count issues failed: {e}"))?;

            let sql = format!(
                r#"{ISSUE_SELECT_PG}
WHERE repo_id = $1
  AND ($2::text IS NULL OR state = $2)
  AND ($3::text IS NULL OR author_id = $3)
  AND ($4::text IS NULL OR EXISTS (
        SELECT 1 FROM issue_labels il WHERE il.issue_id = issues.id AND il.label_id = $4))
  AND ($5::text IS NULL OR EXISTS (
        SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = issues.id AND ia.user_id = $5))
  AND ($6::text IS NULL OR title ILIKE $6 OR body ILIKE $6)
ORDER BY updated_at DESC
OFFSET $7 LIMIT $8"#
            );
            let rows = sqlx::query(&sql)
                .bind(repo_id)
                .bind(state)
                .bind(author_id)
                .bind(label_id)
                .bind(assignee_id)
                .bind(q.as_deref())
                .bind(offset)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issues failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_issue!(&r));
            }
            Ok((out, total))
        }
        DbPool::MySql(p) => {
            let total: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM issues
WHERE repo_id = ?
  AND (? IS NULL OR state = ?)
  AND (? IS NULL OR author_id = ?)
  AND (? IS NULL OR EXISTS (
        SELECT 1 FROM issue_labels il WHERE il.issue_id = issues.id AND il.label_id = ?))
  AND (? IS NULL OR EXISTS (
        SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = issues.id AND ia.user_id = ?))
  AND (? IS NULL OR title LIKE ? OR body LIKE ?)"#,
            )
            .bind(repo_id)
            .bind(state)
            .bind(state)
            .bind(author_id)
            .bind(author_id)
            .bind(label_id)
            .bind(label_id)
            .bind(assignee_id)
            .bind(assignee_id)
            .bind(q.as_deref())
            .bind(q.as_deref())
            .bind(q.as_deref())
            .fetch_one(p)
            .await
            .map_err(|e| format!("count issues failed: {e}"))?;

            let sql = format!(
                r#"{ISSUE_SELECT_MYSQL}
WHERE repo_id = ?
  AND (? IS NULL OR state = ?)
  AND (? IS NULL OR author_id = ?)
  AND (? IS NULL OR EXISTS (
        SELECT 1 FROM issue_labels il WHERE il.issue_id = issues.id AND il.label_id = ?))
  AND (? IS NULL OR EXISTS (
        SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = issues.id AND ia.user_id = ?))
  AND (? IS NULL OR title LIKE ? OR body LIKE ?)
ORDER BY updated_at DESC
LIMIT ? OFFSET ?"#
            );
            let rows = sqlx::query(&sql)
                .bind(repo_id)
                .bind(state)
                .bind(state)
                .bind(author_id)
                .bind(author_id)
                .bind(label_id)
                .bind(label_id)
                .bind(assignee_id)
                .bind(assignee_id)
                .bind(q.as_deref())
                .bind(q.as_deref())
                .bind(q.as_deref())
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issues failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_issue!(&r));
            }
            Ok((out, total))
        }
        DbPool::Sqlite(p) => {
            let total: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM issues
WHERE repo_id = ?1
  AND (?2 IS NULL OR state = ?2)
  AND (?3 IS NULL OR author_id = ?3)
  AND (?4 IS NULL OR EXISTS (
        SELECT 1 FROM issue_labels il WHERE il.issue_id = issues.id AND il.label_id = ?4))
  AND (?5 IS NULL OR EXISTS (
        SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = issues.id AND ia.user_id = ?5))
  AND (?6 IS NULL OR title LIKE ?6 COLLATE NOCASE OR body LIKE ?6 COLLATE NOCASE)"#,
            )
            .bind(repo_id)
            .bind(state)
            .bind(author_id)
            .bind(label_id)
            .bind(assignee_id)
            .bind(q.as_deref())
            .fetch_one(p)
            .await
            .map_err(|e| format!("count issues failed: {e}"))?;

            let sql = format!(
                r#"{ISSUE_SELECT_SQLITE}
WHERE repo_id = ?1
  AND (?2 IS NULL OR state = ?2)
  AND (?3 IS NULL OR author_id = ?3)
  AND (?4 IS NULL OR EXISTS (
        SELECT 1 FROM issue_labels il WHERE il.issue_id = issues.id AND il.label_id = ?4))
  AND (?5 IS NULL OR EXISTS (
        SELECT 1 FROM issue_assignees ia WHERE ia.issue_id = issues.id AND ia.user_id = ?5))
  AND (?6 IS NULL OR title LIKE ?6 COLLATE NOCASE OR body LIKE ?6 COLLATE NOCASE)
ORDER BY updated_at DESC
LIMIT ?7 OFFSET ?8"#
            );
            let rows = sqlx::query(&sql)
                .bind(repo_id)
                .bind(state)
                .bind(author_id)
                .bind(label_id)
                .bind(assignee_id)
                .bind(q.as_deref())
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issues failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_issue!(&r));
            }
            Ok((out, total))
        }
    }
}

/// Open-issue count for repo chrome badges (cheap COUNT, no filters).
pub async fn count_open_issues_for_repo(pool: &DbPool, repo_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*)::bigint FROM issues WHERE repo_id = $1 AND state = 'open'",
        )
        .bind(repo_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count open issues failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM issues WHERE repo_id = ? AND state = 'open'",
        )
        .bind(repo_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count open issues failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM issues WHERE repo_id = ?1 AND state = 'open'",
        )
        .bind(repo_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count open issues failed: {e}")),
    }
}

/// Hard-delete an issue. Does **not** decrement `issue_counters` (D-ISS-01 / D-ISS-02).
pub async fn delete_issue(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM issues WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM issues WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM issues WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue failed: {e}"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct IssueRevisionRow {
    pub id: String,
    pub issue_id: String,
    pub editor_id: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
}

macro_rules! map_revision {
    ($row:expr) => {{
        let row = $row;
        IssueRevisionRow {
            id: row.try_get("id").map_err(|e| format!("revision row: {e}"))?,
            issue_id: row
                .try_get("issue_id")
                .map_err(|e| format!("revision row: {e}"))?,
            editor_id: row
                .try_get("editor_id")
                .map_err(|e| format!("revision row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("revision row: {e}"))?,
            body: row.try_get("body").map_err(|e| format!("revision row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("revision row: {e}"))?,
        }
    }};
}

const REV_SELECT_PG: &str = "SELECT id, issue_id, editor_id, title, body,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM issue_revisions";

const REV_SELECT_MYSQL: &str = "SELECT id, issue_id, editor_id, title, body,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM issue_revisions";

const REV_SELECT_SQLITE: &str = "SELECT id, issue_id, editor_id, title, body,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM issue_revisions";

/// Insert a prior title/body snapshot (D-ISS-04). Call before applying the new values.
pub async fn insert_issue_revision(
    pool: &DbPool,
    id: &str,
    issue_id: &str,
    editor_id: &str,
    title: &str,
    body: &str,
) -> Result<IssueRevisionRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO issue_revisions (id, issue_id, editor_id, title, body)
VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(id)
            .bind(issue_id)
            .bind(editor_id)
            .bind(title)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue revision failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO issue_revisions (id, issue_id, editor_id, title, body)
VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(issue_id)
            .bind(editor_id)
            .bind(title)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue revision failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO issue_revisions (id, issue_id, editor_id, title, body)
VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(id)
            .bind(issue_id)
            .bind(editor_id)
            .bind(title)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue revision failed: {e}"))?;
        }
    }
    list_issue_revisions(pool, issue_id)
        .await?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| "insert issue revision failed: row missing after insert".into())
}

/// Revisions oldest-first (chronological trail).
pub async fn list_issue_revisions(
    pool: &DbPool,
    issue_id: &str,
) -> Result<Vec<IssueRevisionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(&format!(
                "{REV_SELECT_PG} WHERE issue_id = $1 ORDER BY created_at ASC, id ASC"
            ))
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue revisions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_revision!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(&format!(
                "{REV_SELECT_MYSQL} WHERE issue_id = ? ORDER BY created_at ASC, id ASC"
            ))
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue revisions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_revision!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(&format!(
                "{REV_SELECT_SQLITE} WHERE issue_id = ?1 ORDER BY created_at ASC, rowid ASC"
            ))
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue revisions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_revision!(&r));
            }
            Ok(out)
        }
    }
}

/// Update title/body and bump `updated_at`.
pub async fn update_issue_content(
    pool: &DbPool,
    id: &str,
    title: &str,
    body: &str,
) -> Result<IssueRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE issues SET title = $2, body = $3, updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .bind(title)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("update issue content failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE issues SET title = ?, body = ?, updated_at = UTC_TIMESTAMP() WHERE id = ?",
            )
            .bind(title)
            .bind(body)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update issue content failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE issues SET title = ?2, body = ?3,
 updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?1",
            )
            .bind(id)
            .bind(title)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("update issue content failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "update issue content failed: row missing".into())
}

/// Close an open issue.
pub async fn close_issue(
    pool: &DbPool,
    id: &str,
    closed_by: &str,
) -> Result<IssueRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE issues SET state = 'closed', closed_at = NOW(), closed_by = $2,
 updated_at = NOW() WHERE id = $1 AND state = 'open'",
            )
            .bind(id)
            .bind(closed_by)
            .execute(p)
            .await
            .map_err(|e| format!("close issue failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE issues SET state = 'closed', closed_at = UTC_TIMESTAMP(), closed_by = ?,
 updated_at = UTC_TIMESTAMP() WHERE id = ? AND state = 'open'",
            )
            .bind(closed_by)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("close issue failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE issues SET state = 'closed',
 closed_at = strftime('%Y-%m-%d %H:%M:%S','now'), closed_by = ?2,
 updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
 WHERE id = ?1 AND state = 'open'",
            )
            .bind(id)
            .bind(closed_by)
            .execute(p)
            .await
            .map_err(|e| format!("close issue failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "close issue failed: row missing".into())
}

/// Reopen a closed issue.
pub async fn reopen_issue(pool: &DbPool, id: &str) -> Result<IssueRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE issues SET state = 'open', closed_at = NULL, closed_by = NULL,
 updated_at = NOW() WHERE id = $1 AND state = 'closed'",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("reopen issue failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE issues SET state = 'open', closed_at = NULL, closed_by = NULL,
 updated_at = UTC_TIMESTAMP() WHERE id = ? AND state = 'closed'",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("reopen issue failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE issues SET state = 'open', closed_at = NULL, closed_by = NULL,
 updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
 WHERE id = ?1 AND state = 'closed'",
            )
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("reopen issue failed: {e}"))?;
        }
    }
    find_by_id(pool, id)
        .await?
        .ok_or_else(|| "reopen issue failed: row missing".into())
}

#[derive(Debug, Clone)]
pub struct IssueCommentRow {
    pub id: String,
    pub issue_id: String,
    pub author_id: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

macro_rules! map_comment {
    ($row:expr) => {{
        let row = $row;
        IssueCommentRow {
            id: row.try_get("id").map_err(|e| format!("comment row: {e}"))?,
            issue_id: row
                .try_get("issue_id")
                .map_err(|e| format!("comment row: {e}"))?,
            author_id: row
                .try_get("author_id")
                .map_err(|e| format!("comment row: {e}"))?,
            body: row.try_get("body").map_err(|e| format!("comment row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("comment row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("comment row: {e}"))?,
        }
    }};
}

const COMMENT_SELECT_PG: &str = "SELECT id, issue_id, author_id, body,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       to_char(updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at
FROM issue_comments";

const COMMENT_SELECT_MYSQL: &str = "SELECT id, issue_id, author_id, body,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       DATE_FORMAT(updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at
FROM issue_comments";

const COMMENT_SELECT_SQLITE: &str = "SELECT id, issue_id, author_id, body,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at,
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) AS updated_at
FROM issue_comments";

pub async fn insert_issue_comment(
    pool: &DbPool,
    id: &str,
    issue_id: &str,
    author_id: &str,
    body: &str,
) -> Result<IssueCommentRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO issue_comments (id, issue_id, author_id, body)
VALUES ($1, $2, $3, $4)",
            )
            .bind(id)
            .bind(issue_id)
            .bind(author_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue comment failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO issue_comments (id, issue_id, author_id, body)
VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(issue_id)
            .bind(author_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue comment failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO issue_comments (id, issue_id, author_id, body)
VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(id)
            .bind(issue_id)
            .bind(author_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue comment failed: {e}"))?;
        }
    }
    find_issue_comment_by_id(pool, id)
        .await?
        .ok_or_else(|| "insert issue comment failed: row missing after insert".into())
}

pub async fn find_issue_comment_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<IssueCommentRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!("{COMMENT_SELECT_PG} WHERE id = $1");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue comment failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_comment!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let q = format!("{COMMENT_SELECT_MYSQL} WHERE id = ?");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue comment failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_comment!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let q = format!("{COMMENT_SELECT_SQLITE} WHERE id = ?1");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue comment failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_comment!(&r)),
                None => None,
            })
        }
    }
}

pub async fn list_issue_comments(
    pool: &DbPool,
    issue_id: &str,
) -> Result<Vec<IssueCommentRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!("{COMMENT_SELECT_PG} WHERE issue_id = $1 ORDER BY created_at ASC, id ASC");
            let rows = sqlx::query(&q)
                .bind(issue_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue comments failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_comment!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let q = format!("{COMMENT_SELECT_MYSQL} WHERE issue_id = ? ORDER BY created_at ASC, id ASC");
            let rows = sqlx::query(&q)
                .bind(issue_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue comments failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_comment!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let q = format!(
                "{COMMENT_SELECT_SQLITE} WHERE issue_id = ?1 ORDER BY created_at ASC, id ASC"
            );
            let rows = sqlx::query(&q)
                .bind(issue_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue comments failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_comment!(&r));
            }
            Ok(out)
        }
    }
}

/// Distinct comment author ids for an issue — participant fan-out needs only
/// the ids, not the comment bodies.
pub async fn list_issue_comment_author_ids(
    pool: &DbPool,
    issue_id: &str,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT author_id FROM issue_comments WHERE issue_id = $1",
            )
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue comment author ids failed: {e}"))
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT author_id FROM issue_comments WHERE issue_id = ?",
            )
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue comment author ids failed: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar::<_, String>(
                "SELECT DISTINCT author_id FROM issue_comments WHERE issue_id = ?1",
            )
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue comment author ids failed: {e}"))
        }
    }
}

pub async fn update_issue_comment_body(
    pool: &DbPool,
    id: &str,
    body: &str,
) -> Result<IssueCommentRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE issue_comments SET body = $2, updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("update issue comment failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE issue_comments SET body = ?, updated_at = UTC_TIMESTAMP() WHERE id = ?",
            )
            .bind(body)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("update issue comment failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE issue_comments SET body = ?2,
 updated_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?1",
            )
            .bind(id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("update issue comment failed: {e}"))?;
        }
    }
    find_issue_comment_by_id(pool, id)
        .await?
        .ok_or_else(|| "update issue comment failed: row missing".into())
}

pub async fn delete_issue_comment(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM issue_comments WHERE id = $1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue comment failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM issue_comments WHERE id = ?")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue comment failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM issue_comments WHERE id = ?1")
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue comment failed: {e}"))?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CommentRevisionRow {
    pub id: String,
    pub comment_id: String,
    pub editor_id: String,
    pub body: String,
    pub created_at: String,
}

macro_rules! map_comment_revision {
    ($row:expr) => {{
        let row = $row;
        CommentRevisionRow {
            id: row.try_get("id").map_err(|e| format!("comment revision row: {e}"))?,
            comment_id: row
                .try_get("comment_id")
                .map_err(|e| format!("comment revision row: {e}"))?,
            editor_id: row
                .try_get("editor_id")
                .map_err(|e| format!("comment revision row: {e}"))?,
            body: row
                .try_get("body")
                .map_err(|e| format!("comment revision row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("comment revision row: {e}"))?,
        }
    }};
}

const COMMENT_REV_SELECT_PG: &str = "SELECT id, comment_id, editor_id, body,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM comment_revisions";

const COMMENT_REV_SELECT_MYSQL: &str = "SELECT id, comment_id, editor_id, body,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM comment_revisions";

const COMMENT_REV_SELECT_SQLITE: &str = "SELECT id, comment_id, editor_id, body,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM comment_revisions";

/// Insert a prior comment body snapshot (D-ISS-12). Call before applying the new body.
pub async fn insert_comment_revision(
    pool: &DbPool,
    id: &str,
    comment_id: &str,
    editor_id: &str,
    body: &str,
) -> Result<CommentRevisionRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO comment_revisions (id, comment_id, editor_id, body, created_at)
VALUES ($1, $2, $3, $4, NOW())",
            )
            .bind(id)
            .bind(comment_id)
            .bind(editor_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert comment revision failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO comment_revisions (id, comment_id, editor_id, body, created_at)
VALUES (?, ?, ?, ?, UTC_TIMESTAMP(6))",
            )
            .bind(id)
            .bind(comment_id)
            .bind(editor_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert comment revision failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            // Fractional seconds so rapid edits stay oldest-first under ORDER BY created_at.
            sqlx::query(
                "INSERT INTO comment_revisions (id, comment_id, editor_id, body, created_at)
VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%d %H:%M:%f','now'))",
            )
            .bind(id)
            .bind(comment_id)
            .bind(editor_id)
            .bind(body)
            .execute(p)
            .await
            .map_err(|e| format!("insert comment revision failed: {e}"))?;
        }
    }
    list_comment_revisions(pool, comment_id)
        .await?
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| "insert comment revision failed: row missing after insert".into())
}

pub async fn list_comment_revisions(
    pool: &DbPool,
    comment_id: &str,
) -> Result<Vec<CommentRevisionRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!(
                "{COMMENT_REV_SELECT_PG} WHERE comment_id = $1 ORDER BY created_at ASC, id ASC"
            );
            let rows = sqlx::query(&q)
                .bind(comment_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list comment revisions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_comment_revision!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let q = format!(
                "{COMMENT_REV_SELECT_MYSQL} WHERE comment_id = ? ORDER BY created_at ASC, id ASC"
            );
            let rows = sqlx::query(&q)
                .bind(comment_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list comment revisions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_comment_revision!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let q = format!(
                "{COMMENT_REV_SELECT_SQLITE} WHERE comment_id = ?1 ORDER BY created_at ASC, rowid ASC"
            );
            let rows = sqlx::query(&q)
                .bind(comment_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list comment revisions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_comment_revision!(&r));
            }
            Ok(out)
        }
    }
}

/// Aggregated reaction group for an issue or comment (D-ISS-11).
#[derive(Debug, Clone)]
pub struct ReactionGroupRow {
    pub content: String,
    pub count: i64,
    pub viewer_has_reacted: bool,
}

macro_rules! map_reaction_group_any {
    ($row:expr) => {{
        let row = $row;
        ReactionGroupRow {
            content: row
                .try_get("content")
                .map_err(|e| format!("reaction group: {e}"))?,
            count: {
                let c: i64 = row
                    .try_get("count")
                    .or_else(|_| {
                        row.try_get::<i32, _>("count")
                            .map(|v| i64::from(v))
                    })
                    .map_err(|e| format!("reaction group: {e}"))?;
                c
            },
            viewer_has_reacted: {
                let hit: i64 = row
                    .try_get("viewer_hit")
                    .or_else(|_| {
                        row.try_get::<i32, _>("viewer_hit")
                            .map(|v| i64::from(v))
                    })
                    .or_else(|_| {
                        row.try_get::<bool, _>("viewer_hit")
                            .map(|v| if v { 1 } else { 0 })
                    })
                    .map_err(|e| format!("reaction group: {e}"))?;
                hit > 0
            },
        }
    }};
}

/// List aggregated reaction groups for an issue.
pub async fn list_issue_reaction_groups(
    pool: &DbPool,
    issue_id: &str,
    viewer_user_id: Option<&str>,
) -> Result<Vec<ReactionGroupRow>, String> {
    let viewer = viewer_user_id.unwrap_or("");
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT content,
                          COUNT(*)::bigint AS count,
                          COALESCE(SUM(CASE WHEN user_id = $2 THEN 1 ELSE 0 END), 0)::bigint AS viewer_hit
                   FROM issue_reactions
                   WHERE issue_id = $1
                   GROUP BY content
                   ORDER BY content ASC"#,
            )
            .bind(issue_id)
            .bind(viewer)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue reactions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_reaction_group_any!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ? THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM issue_reactions
                   WHERE issue_id = ?
                   GROUP BY content
                   ORDER BY content ASC"#,
            )
            .bind(viewer)
            .bind(issue_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue reactions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_reaction_group_any!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ?2 THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM issue_reactions
                   WHERE issue_id = ?1
                   GROUP BY content
                   ORDER BY content ASC"#,
            )
            .bind(issue_id)
            .bind(viewer)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue reactions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_reaction_group_any!(&r));
            }
            Ok(out)
        }
    }
}

/// Batch `list_issue_reaction_groups` — one `IN (...)` round trip; `(issue_id, group)` pairs.
pub async fn list_issue_reaction_groups_for_issues(
    pool: &DbPool,
    issue_ids: &[String],
    viewer_user_id: Option<&str>,
) -> Result<Vec<(String, ReactionGroupRow)>, String> {
    if issue_ids.is_empty() {
        return Ok(Vec::new());
    }
    let viewer = viewer_user_id.unwrap_or("");
    macro_rules! map_pair {
        ($rows:expr) => {
            $rows
                .into_iter()
                .map(|r| {
                    Ok::<(String, ReactionGroupRow), String>((
                        r.try_get("issue_id")
                            .map_err(|e| format!("reaction group: {e}"))?,
                        map_reaction_group_any!(&r),
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
        };
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT issue_id, content,
                          COUNT(*)::bigint AS count,
                          COALESCE(SUM(CASE WHEN user_id = $2 THEN 1 ELSE 0 END), 0)::bigint AS viewer_hit
                   FROM issue_reactions
                   WHERE issue_id = ANY($1)
                   GROUP BY issue_id, content
                   ORDER BY issue_id, content ASC"#,
            )
            .bind(issue_ids)
            .bind(viewer)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list issue reactions failed: {e}"))?;
            map_pair!(rows)
        }
        DbPool::MySql(p) => {
            let in_list =
                crate::dialect::in_placeholders(crate::dialect::Dialect::MySql, 2, issue_ids.len());
            let q_str = format!(
                r#"SELECT issue_id, content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ? THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM issue_reactions
                   WHERE issue_id IN ({in_list})
                   GROUP BY issue_id, content
                   ORDER BY issue_id, content ASC"#
            );
            let q = sqlx::query(&q_str);
            let q = q.bind(viewer);
            let q = issue_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue reactions failed: {e}"))?;
            map_pair!(rows)
        }
        DbPool::Sqlite(p) => {
            let in_list = crate::dialect::in_placeholders(
                crate::dialect::Dialect::Sqlite,
                2,
                issue_ids.len(),
            );
            let q_str = format!(
                r#"SELECT issue_id, content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ?1 THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM issue_reactions
                   WHERE issue_id IN ({in_list})
                   GROUP BY issue_id, content
                   ORDER BY issue_id, content ASC"#
            );
            let q = sqlx::query(&q_str);
            let q = q.bind(viewer);
            let q = issue_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue reactions failed: {e}"))?;
            map_pair!(rows)
        }
    }
}

/// List aggregated reaction groups for a comment.
pub async fn list_comment_reaction_groups(
    pool: &DbPool,
    comment_id: &str,
    viewer_user_id: Option<&str>,
) -> Result<Vec<ReactionGroupRow>, String> {
    let viewer = viewer_user_id.unwrap_or("");
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT content,
                          COUNT(*)::bigint AS count,
                          COALESCE(SUM(CASE WHEN user_id = $2 THEN 1 ELSE 0 END), 0)::bigint AS viewer_hit
                   FROM comment_reactions
                   WHERE comment_id = $1
                   GROUP BY content
                   ORDER BY content ASC"#,
            )
            .bind(comment_id)
            .bind(viewer)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list comment reactions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_reaction_group_any!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                r#"SELECT content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ? THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM comment_reactions
                   WHERE comment_id = ?
                   GROUP BY content
                   ORDER BY content ASC"#,
            )
            .bind(viewer)
            .bind(comment_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list comment reactions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_reaction_group_any!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                r#"SELECT content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ?2 THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM comment_reactions
                   WHERE comment_id = ?1
                   GROUP BY content
                   ORDER BY content ASC"#,
            )
            .bind(comment_id)
            .bind(viewer)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list comment reactions failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_reaction_group_any!(&r));
            }
            Ok(out)
        }
    }
}

/// Batch `list_comment_reaction_groups` — one `IN (...)` round trip; `(comment_id, group)` pairs.
pub async fn list_comment_reaction_groups_for_comments(
    pool: &DbPool,
    comment_ids: &[String],
    viewer_user_id: Option<&str>,
) -> Result<Vec<(String, ReactionGroupRow)>, String> {
    if comment_ids.is_empty() {
        return Ok(Vec::new());
    }
    let viewer = viewer_user_id.unwrap_or("");
    macro_rules! map_pair {
        ($rows:expr) => {
            $rows
                .into_iter()
                .map(|r| {
                    Ok::<(String, ReactionGroupRow), String>((
                        r.try_get("comment_id")
                            .map_err(|e| format!("reaction group: {e}"))?,
                        map_reaction_group_any!(&r),
                    ))
                })
                .collect::<Result<Vec<_>, String>>()
        };
    }
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                r#"SELECT comment_id, content,
                          COUNT(*)::bigint AS count,
                          COALESCE(SUM(CASE WHEN user_id = $2 THEN 1 ELSE 0 END), 0)::bigint AS viewer_hit
                   FROM comment_reactions
                   WHERE comment_id = ANY($1)
                   GROUP BY comment_id, content
                   ORDER BY comment_id, content ASC"#,
            )
            .bind(comment_ids)
            .bind(viewer)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list comment reactions failed: {e}"))?;
            map_pair!(rows)
        }
        DbPool::MySql(p) => {
            let in_list = crate::dialect::in_placeholders(
                crate::dialect::Dialect::MySql,
                2,
                comment_ids.len(),
            );
            let q_str = format!(
                r#"SELECT comment_id, content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ? THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM comment_reactions
                   WHERE comment_id IN ({in_list})
                   GROUP BY comment_id, content
                   ORDER BY comment_id, content ASC"#
            );
            let q = sqlx::query(&q_str);
            let q = q.bind(viewer);
            let q = comment_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list comment reactions failed: {e}"))?;
            map_pair!(rows)
        }
        DbPool::Sqlite(p) => {
            let in_list = crate::dialect::in_placeholders(
                crate::dialect::Dialect::Sqlite,
                2,
                comment_ids.len(),
            );
            let q_str = format!(
                r#"SELECT comment_id, content,
                          COUNT(*) AS count,
                          COALESCE(SUM(CASE WHEN user_id = ?1 THEN 1 ELSE 0 END), 0) AS viewer_hit
                   FROM comment_reactions
                   WHERE comment_id IN ({in_list})
                   GROUP BY comment_id, content
                   ORDER BY comment_id, content ASC"#
            );
            let q = sqlx::query(&q_str);
            let q = q.bind(viewer);
            let q = comment_ids.iter().fold(q, |q, id| q.bind(id));
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| format!("list comment reactions failed: {e}"))?;
            map_pair!(rows)
        }
    }
}

/// Toggle an issue reaction for a user. Returns `true` if now reacted (inserted).
pub async fn toggle_issue_reaction(
    pool: &DbPool,
    issue_id: &str,
    user_id: &str,
    content: &str,
) -> Result<bool, String> {
    match pool {
        DbPool::Postgres(p) => {
            let existing = sqlx::query(
                "SELECT 1 AS ok FROM issue_reactions WHERE issue_id = $1 AND user_id = $2 AND content = $3",
            )
            .bind(issue_id)
            .bind(user_id)
            .bind(content)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find issue reaction failed: {e}"))?;
            if existing.is_some() {
                sqlx::query(
                    "DELETE FROM issue_reactions WHERE issue_id = $1 AND user_id = $2 AND content = $3",
                )
                .bind(issue_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue reaction failed: {e}"))?;
                Ok(false)
            } else {
                sqlx::query(
                    "INSERT INTO issue_reactions (issue_id, user_id, content) VALUES ($1, $2, $3)",
                )
                .bind(issue_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("insert issue reaction failed: {e}"))?;
                Ok(true)
            }
        }
        DbPool::MySql(p) => {
            let existing = sqlx::query(
                "SELECT 1 AS ok FROM issue_reactions WHERE issue_id = ? AND user_id = ? AND content = ?",
            )
            .bind(issue_id)
            .bind(user_id)
            .bind(content)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find issue reaction failed: {e}"))?;
            if existing.is_some() {
                sqlx::query(
                    "DELETE FROM issue_reactions WHERE issue_id = ? AND user_id = ? AND content = ?",
                )
                .bind(issue_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue reaction failed: {e}"))?;
                Ok(false)
            } else {
                sqlx::query(
                    "INSERT INTO issue_reactions (issue_id, user_id, content) VALUES (?, ?, ?)",
                )
                .bind(issue_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("insert issue reaction failed: {e}"))?;
                Ok(true)
            }
        }
        DbPool::Sqlite(p) => {
            let existing = sqlx::query(
                "SELECT 1 AS ok FROM issue_reactions WHERE issue_id = ?1 AND user_id = ?2 AND content = ?3",
            )
            .bind(issue_id)
            .bind(user_id)
            .bind(content)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find issue reaction failed: {e}"))?;
            if existing.is_some() {
                sqlx::query(
                    "DELETE FROM issue_reactions WHERE issue_id = ?1 AND user_id = ?2 AND content = ?3",
                )
                .bind(issue_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue reaction failed: {e}"))?;
                Ok(false)
            } else {
                sqlx::query(
                    "INSERT INTO issue_reactions (issue_id, user_id, content) VALUES (?1, ?2, ?3)",
                )
                .bind(issue_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("insert issue reaction failed: {e}"))?;
                Ok(true)
            }
        }
    }
}

/// Toggle a comment reaction for a user. Returns `true` if now reacted (inserted).
pub async fn toggle_comment_reaction(
    pool: &DbPool,
    comment_id: &str,
    user_id: &str,
    content: &str,
) -> Result<bool, String> {
    match pool {
        DbPool::Postgres(p) => {
            let existing = sqlx::query(
                "SELECT 1 AS ok FROM comment_reactions WHERE comment_id = $1 AND user_id = $2 AND content = $3",
            )
            .bind(comment_id)
            .bind(user_id)
            .bind(content)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find comment reaction failed: {e}"))?;
            if existing.is_some() {
                sqlx::query(
                    "DELETE FROM comment_reactions WHERE comment_id = $1 AND user_id = $2 AND content = $3",
                )
                .bind(comment_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("delete comment reaction failed: {e}"))?;
                Ok(false)
            } else {
                sqlx::query(
                    "INSERT INTO comment_reactions (comment_id, user_id, content) VALUES ($1, $2, $3)",
                )
                .bind(comment_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("insert comment reaction failed: {e}"))?;
                Ok(true)
            }
        }
        DbPool::MySql(p) => {
            let existing = sqlx::query(
                "SELECT 1 AS ok FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND content = ?",
            )
            .bind(comment_id)
            .bind(user_id)
            .bind(content)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find comment reaction failed: {e}"))?;
            if existing.is_some() {
                sqlx::query(
                    "DELETE FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND content = ?",
                )
                .bind(comment_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("delete comment reaction failed: {e}"))?;
                Ok(false)
            } else {
                sqlx::query(
                    "INSERT INTO comment_reactions (comment_id, user_id, content) VALUES (?, ?, ?)",
                )
                .bind(comment_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("insert comment reaction failed: {e}"))?;
                Ok(true)
            }
        }
        DbPool::Sqlite(p) => {
            let existing = sqlx::query(
                "SELECT 1 AS ok FROM comment_reactions WHERE comment_id = ?1 AND user_id = ?2 AND content = ?3",
            )
            .bind(comment_id)
            .bind(user_id)
            .bind(content)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find comment reaction failed: {e}"))?;
            if existing.is_some() {
                sqlx::query(
                    "DELETE FROM comment_reactions WHERE comment_id = ?1 AND user_id = ?2 AND content = ?3",
                )
                .bind(comment_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("delete comment reaction failed: {e}"))?;
                Ok(false)
            } else {
                sqlx::query(
                    "INSERT INTO comment_reactions (comment_id, user_id, content) VALUES (?1, ?2, ?3)",
                )
                .bind(comment_id)
                .bind(user_id)
                .bind(content)
                .execute(p)
                .await
                .map_err(|e| format!("insert comment reaction failed: {e}"))?;
                Ok(true)
            }
        }
    }
}

/// Linked issue / PR stub row (D-ISS-13).
#[derive(Debug, Clone)]
pub struct IssueLinkRow {
    pub id: String,
    pub issue_id: String,
    pub kind: String,
    pub target_repo_id: Option<String>,
    pub target_number: Option<i64>,
    pub target_opaque_id: Option<String>,
    pub title: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

macro_rules! map_issue_link {
    ($row:expr) => {{
        let row = $row;
        IssueLinkRow {
            id: row.try_get("id").map_err(|e| format!("issue link: {e}"))?,
            issue_id: row
                .try_get("issue_id")
                .map_err(|e| format!("issue link: {e}"))?,
            kind: row.try_get("kind").map_err(|e| format!("issue link: {e}"))?,
            target_repo_id: row
                .try_get("target_repo_id")
                .map_err(|e| format!("issue link: {e}"))?,
            target_number: row
                .try_get::<Option<i64>, _>("target_number")
                .or_else(|_| {
                    row.try_get::<Option<i32>, _>("target_number")
                        .map(|v| v.map(i64::from))
                })
                .map_err(|e| format!("issue link: {e}"))?,
            target_opaque_id: row
                .try_get("target_opaque_id")
                .map_err(|e| format!("issue link: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("issue link: {e}"))?,
            created_by: row
                .try_get("created_by")
                .map_err(|e| format!("issue link: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("issue link: {e}"))?,
        }
    }};
}

const LINK_SELECT_PG: &str = r#"SELECT id, issue_id, kind, target_repo_id, target_number,
       target_opaque_id, title, created_by,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"') AS created_at
FROM issue_links"#;

const LINK_SELECT_MYSQL: &str = r#"SELECT id, issue_id, kind, target_repo_id, target_number,
       target_opaque_id, title, created_by,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM issue_links"#;

const LINK_SELECT_SQLITE: &str = r#"SELECT id, issue_id, kind, target_repo_id, target_number,
       target_opaque_id, title, created_by,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM issue_links"#;

/// Insert a stub or issue link row; returns the stored row.
pub async fn insert_issue_link(
    pool: &DbPool,
    id: &str,
    issue_id: &str,
    kind: &str,
    target_repo_id: Option<&str>,
    target_number: Option<i64>,
    target_opaque_id: Option<&str>,
    title: Option<&str>,
    created_by: &str,
) -> Result<IssueLinkRow, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                r#"INSERT INTO issue_links
                   (id, issue_id, kind, target_repo_id, target_number, target_opaque_id, title, created_by)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            )
            .bind(id)
            .bind(issue_id)
            .bind(kind)
            .bind(target_repo_id)
            .bind(target_number)
            .bind(target_opaque_id)
            .bind(title)
            .bind(created_by)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue link failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                r#"INSERT INTO issue_links
                   (id, issue_id, kind, target_repo_id, target_number, target_opaque_id, title, created_by)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(id)
            .bind(issue_id)
            .bind(kind)
            .bind(target_repo_id)
            .bind(target_number)
            .bind(target_opaque_id)
            .bind(title)
            .bind(created_by)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue link failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                r#"INSERT INTO issue_links
                   (id, issue_id, kind, target_repo_id, target_number, target_opaque_id, title, created_by)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"#,
            )
            .bind(id)
            .bind(issue_id)
            .bind(kind)
            .bind(target_repo_id)
            .bind(target_number)
            .bind(target_opaque_id)
            .bind(title)
            .bind(created_by)
            .execute(p)
            .await
            .map_err(|e| format!("insert issue link failed: {e}"))?;
        }
    }
    find_issue_link_by_id(pool, id)
        .await?
        .ok_or_else(|| "issue link missing after insert".to_string())
}

pub async fn find_issue_link_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<IssueLinkRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!("{LINK_SELECT_PG} WHERE id = $1");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue link failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue_link!(&r)),
                None => None,
            })
        }
        DbPool::MySql(p) => {
            let q = format!("{LINK_SELECT_MYSQL} WHERE id = ?");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue link failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue_link!(&r)),
                None => None,
            })
        }
        DbPool::Sqlite(p) => {
            let q = format!("{LINK_SELECT_SQLITE} WHERE id = ?1");
            let row = sqlx::query(&q)
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find issue link failed: {e}"))?;
            Ok(match row {
                Some(r) => Some(map_issue_link!(&r)),
                None => None,
            })
        }
    }
}

pub async fn list_issue_links(
    pool: &DbPool,
    issue_id: &str,
) -> Result<Vec<IssueLinkRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let q = format!("{LINK_SELECT_PG} WHERE issue_id = $1 ORDER BY created_at ASC, id ASC");
            let rows = sqlx::query(&q)
                .bind(issue_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue links failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_issue_link!(&r));
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let q = format!("{LINK_SELECT_MYSQL} WHERE issue_id = ? ORDER BY created_at ASC, id ASC");
            let rows = sqlx::query(&q)
                .bind(issue_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue links failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_issue_link!(&r));
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let q = format!(
                "{LINK_SELECT_SQLITE} WHERE issue_id = ?1 ORDER BY created_at ASC, id ASC"
            );
            let rows = sqlx::query(&q)
                .bind(issue_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list issue links failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_issue_link!(&r));
            }
            Ok(out)
        }
    }
}

/// Delete a link belonging to `issue_id`. Returns true if a row was deleted.
pub async fn delete_issue_link(
    pool: &DbPool,
    issue_id: &str,
    link_id: &str,
) -> Result<bool, String> {
    match pool {
        DbPool::Postgres(p) => {
            let res = sqlx::query("DELETE FROM issue_links WHERE id = $1 AND issue_id = $2")
                .bind(link_id)
                .bind(issue_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue link failed: {e}"))?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::MySql(p) => {
            let res = sqlx::query("DELETE FROM issue_links WHERE id = ? AND issue_id = ?")
                .bind(link_id)
                .bind(issue_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue link failed: {e}"))?;
            Ok(res.rows_affected() > 0)
        }
        DbPool::Sqlite(p) => {
            let res = sqlx::query("DELETE FROM issue_links WHERE id = ?1 AND issue_id = ?2")
                .bind(link_id)
                .bind(issue_id)
                .execute(p)
                .await
                .map_err(|e| format!("delete issue link failed: {e}"))?;
            Ok(res.rows_affected() > 0)
        }
    }
}
