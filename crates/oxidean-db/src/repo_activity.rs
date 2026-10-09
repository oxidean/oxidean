//! Repository activity feed rows (push / branch / merge events).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct RepoActivityRow {
    pub id: String,
    pub repository_id: String,
    pub actor_id: String,
    pub push_type: String,
    pub ref_name: String,
    pub before_oid: String,
    pub after_oid: String,
    pub commits_count: i64,
    pub commit_message: Option<String>,
    pub pr_number: Option<i64>,
    pub created_at: String,
    /// Joined from users on list queries.
    pub actor_username: String,
    pub actor_display_name: String,
    pub actor_avatar_path: Option<String>,
    /// Joined from repositories on actor-scoped queries (`list_by_actor`).
    pub repo_name: Option<String>,
    /// Owner slug/username for `owner/name` links (`list_by_actor`).
    pub repo_owner: Option<String>,
}

macro_rules! map_activity {
    ($row:expr) => {{
        let row = $row;
        let commits_count: i64 = row
            .try_get::<i64, _>("commits_count")
            .or_else(|_| row.try_get::<i32, _>("commits_count").map(|v| i64::from(v)))
            .map_err(|e| format!("activity row: {e}"))?;
        let pr_number: Option<i64> = row
            .try_get::<Option<i64>, _>("pr_number")
            .or_else(|_| {
                row.try_get::<Option<i32>, _>("pr_number")
                    .map(|o| o.map(i64::from))
            })
            .unwrap_or(None);
        RepoActivityRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("activity row: {e}"))?,
            repository_id: row
                .try_get("repository_id")
                .map_err(|e| format!("activity row: {e}"))?,
            actor_id: row
                .try_get("actor_id")
                .map_err(|e| format!("activity row: {e}"))?,
            push_type: row
                .try_get("push_type")
                .map_err(|e| format!("activity row: {e}"))?,
            ref_name: row
                .try_get("ref_name")
                .map_err(|e| format!("activity row: {e}"))?,
            before_oid: row
                .try_get("before_oid")
                .map_err(|e| format!("activity row: {e}"))?,
            after_oid: row
                .try_get("after_oid")
                .map_err(|e| format!("activity row: {e}"))?,
            commits_count,
            commit_message: row
                .try_get::<Option<String>, _>("commit_message")
                .unwrap_or(None),
            pr_number,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("activity row: {e}"))?,
            actor_username: row
                .try_get("actor_username")
                .map_err(|e| format!("activity row: {e}"))?,
            actor_display_name: row
                .try_get("actor_display_name")
                .map_err(|e| format!("activity row: {e}"))?,
            actor_avatar_path: row
                .try_get::<Option<String>, _>("actor_avatar_path")
                .unwrap_or(None),
            repo_name: row
                .try_get::<Option<String>, _>("repo_name")
                .unwrap_or(None),
            repo_owner: row
                .try_get::<Option<String>, _>("repo_owner")
                .unwrap_or(None),
        }
    }};
}

const SELECT_PG: &str = "SELECT a.id, a.repository_id, a.actor_id, a.push_type, a.ref_name,
       a.before_oid, a.after_oid, a.commits_count, a.commit_message, a.pr_number,
       to_char(a.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       u.username AS actor_username, u.display_name AS actor_display_name,
       u.avatar_path AS actor_avatar_path
FROM repository_activity a
INNER JOIN users u ON u.id = a.actor_id";

const SELECT_MYSQL: &str = "SELECT a.id, a.repository_id, a.actor_id, a.push_type, a.ref_name,
       a.before_oid, a.after_oid, a.commits_count, a.commit_message, a.pr_number,
       DATE_FORMAT(a.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       u.username AS actor_username, u.display_name AS actor_display_name,
       u.avatar_path AS actor_avatar_path
FROM repository_activity a
INNER JOIN users u ON u.id = a.actor_id";

const SELECT_SQLITE: &str = "SELECT a.id, a.repository_id, a.actor_id, a.push_type, a.ref_name,
       a.before_oid, a.after_oid, a.commits_count, a.commit_message, a.pr_number,
       strftime('%Y-%m-%dT%H:%M:%SZ', a.created_at) AS created_at,
       u.username AS actor_username, u.display_name AS actor_display_name,
       u.avatar_path AS actor_avatar_path
FROM repository_activity a
INNER JOIN users u ON u.id = a.actor_id";

/// Actor-scoped select: joins repositories so the admin user-activity view can
/// show `owner/name` without a second round trip.
const ACTOR_SELECT_PG: &str = "SELECT a.id, a.repository_id, a.actor_id, a.push_type, a.ref_name,
       a.before_oid, a.after_oid, a.commits_count, a.commit_message, a.pr_number,
       to_char(a.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
       u.username AS actor_username, u.display_name AS actor_display_name,
       u.avatar_path AS actor_avatar_path,
       r.name AS repo_name,
       COALESCE(ru.username, ro.slug) AS repo_owner
FROM repository_activity a
INNER JOIN users u ON u.id = a.actor_id
INNER JOIN repositories r ON r.id = a.repository_id
LEFT JOIN users ru ON r.owner_type = 'user' AND ru.id = r.owner_id
LEFT JOIN organizations ro ON r.owner_type = 'org' AND ro.id = r.owner_id";

const ACTOR_SELECT_MYSQL: &str =
    "SELECT a.id, a.repository_id, a.actor_id, a.push_type, a.ref_name,
       a.before_oid, a.after_oid, a.commits_count, a.commit_message, a.pr_number,
       DATE_FORMAT(a.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
       u.username AS actor_username, u.display_name AS actor_display_name,
       u.avatar_path AS actor_avatar_path,
       r.name AS repo_name,
       COALESCE(ru.username, ro.slug) AS repo_owner
FROM repository_activity a
INNER JOIN users u ON u.id = a.actor_id
INNER JOIN repositories r ON r.id = a.repository_id
LEFT JOIN users ru ON r.owner_type = 'user' AND ru.id = r.owner_id
LEFT JOIN organizations ro ON r.owner_type = 'org' AND ro.id = r.owner_id";

const ACTOR_SELECT_SQLITE: &str =
    "SELECT a.id, a.repository_id, a.actor_id, a.push_type, a.ref_name,
       a.before_oid, a.after_oid, a.commits_count, a.commit_message, a.pr_number,
       strftime('%Y-%m-%dT%H:%M:%SZ', a.created_at) AS created_at,
       u.username AS actor_username, u.display_name AS actor_display_name,
       u.avatar_path AS actor_avatar_path,
       r.name AS repo_name,
       COALESCE(ru.username, ro.slug) AS repo_owner
FROM repository_activity a
INNER JOIN users u ON u.id = a.actor_id
INNER JOIN repositories r ON r.id = a.repository_id
LEFT JOIN users ru ON r.owner_type = 'user' AND ru.id = r.owner_id
LEFT JOIN organizations ro ON r.owner_type = 'org' AND ro.id = r.owner_id";

/// Repo activity authored by a user, newest first (admin user-activity view).
pub async fn list_by_actor(
    pool: &DbPool,
    actor_id: &str,
    limit: i64,
) -> Result<Vec<RepoActivityRow>, String> {
    list_by_actor_impl(pool, actor_id, limit, false).await
}

/// Public-repo-only variant for user-facing Atom feeds (API-05): activity in
/// private repositories must never leak into a feed anyone can subscribe to.
pub async fn list_by_actor_public(
    pool: &DbPool,
    actor_id: &str,
    limit: i64,
) -> Result<Vec<RepoActivityRow>, String> {
    list_by_actor_impl(pool, actor_id, limit, true).await
}

async fn list_by_actor_impl(
    pool: &DbPool,
    actor_id: &str,
    limit: i64,
    public_only: bool,
) -> Result<Vec<RepoActivityRow>, String> {
    let limit = limit.clamp(1, 500);
    let vis = if public_only {
        "\n   AND r.visibility = 'public'"
    } else {
        ""
    };
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{ACTOR_SELECT_PG}
 WHERE a.actor_id = $1{vis}
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT $2"
            )))
            .bind(actor_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repository_activity by actor failed: {e}"))?;
            rows.iter().map(|r| Ok(map_activity!(r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{ACTOR_SELECT_MYSQL}
 WHERE a.actor_id = ?{vis}
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ?"
            )))
            .bind(actor_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repository_activity by actor failed: {e}"))?;
            rows.iter().map(|r| Ok(map_activity!(r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{ACTOR_SELECT_SQLITE}
 WHERE a.actor_id = ?1{vis}
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ?2"
            )))
            .bind(actor_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list repository_activity by actor failed: {e}"))?;
            rows.iter().map(|r| Ok(map_activity!(r))).collect()
        }
    }
}

pub struct InsertRepoActivity<'a> {
    pub id: &'a str,
    pub repository_id: &'a str,
    pub actor_id: &'a str,
    pub push_type: &'a str,
    pub ref_name: &'a str,
    pub before_oid: &'a str,
    pub after_oid: &'a str,
    pub commits_count: i64,
    pub commit_message: Option<&'a str>,
    pub pr_number: Option<i64>,
}

pub async fn insert_activity(pool: &DbPool, row: InsertRepoActivity<'_>) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO repository_activity
 (id, repository_id, actor_id, push_type, ref_name, before_oid, after_oid,
  commits_count, commit_message, pr_number)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(row.id)
            .bind(row.repository_id)
            .bind(row.actor_id)
            .bind(row.push_type)
            .bind(row.ref_name)
            .bind(row.before_oid)
            .bind(row.after_oid)
            .bind(row.commits_count)
            .bind(row.commit_message)
            .bind(row.pr_number)
            .execute(p)
            .await
            .map_err(|e| format!("insert repository_activity failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO repository_activity
 (id, repository_id, actor_id, push_type, ref_name, before_oid, after_oid,
  commits_count, commit_message, pr_number)
 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(row.id)
            .bind(row.repository_id)
            .bind(row.actor_id)
            .bind(row.push_type)
            .bind(row.ref_name)
            .bind(row.before_oid)
            .bind(row.after_oid)
            .bind(row.commits_count)
            .bind(row.commit_message)
            .bind(row.pr_number)
            .execute(p)
            .await
            .map_err(|e| format!("insert repository_activity failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO repository_activity
 (id, repository_id, actor_id, push_type, ref_name, before_oid, after_oid,
  commits_count, commit_message, pr_number)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .bind(row.id)
            .bind(row.repository_id)
            .bind(row.actor_id)
            .bind(row.push_type)
            .bind(row.ref_name)
            .bind(row.before_oid)
            .bind(row.after_oid)
            .bind(row.commits_count)
            .bind(row.commit_message)
            .bind(row.pr_number)
            .execute(p)
            .await
            .map_err(|e| format!("insert repository_activity failed: {e}"))?;
        }
    }
    Ok(())
}

/// List activity for a repo, newest first. Optional `push_type` and `since` (ISO-ish UTC).
pub async fn list_activity(
    pool: &DbPool,
    repository_id: &str,
    push_type: Option<&str>,
    since: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<(Vec<RepoActivityRow>, i64), String> {
    let offset = offset.max(0);
    let limit = limit.clamp(1, 100);
    match pool {
        DbPool::Postgres(p) => {
            let total: i64 = match (push_type, since) {
                (Some(pt), Some(s)) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = $1 AND push_type = $2 AND created_at >= $3::timestamptz",
                    )
                    .bind(repository_id)
                    .bind(pt)
                    .bind(s)
                    .fetch_one(p)
                    .await
                }
                (Some(pt), None) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = $1 AND push_type = $2",
                    )
                    .bind(repository_id)
                    .bind(pt)
                    .fetch_one(p)
                    .await
                }
                (None, Some(s)) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = $1 AND created_at >= $2::timestamptz",
                    )
                    .bind(repository_id)
                    .bind(s)
                    .fetch_one(p)
                    .await
                }
                (None, None) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity WHERE repository_id = $1",
                    )
                    .bind(repository_id)
                    .fetch_one(p)
                    .await
                }
            }
            .map_err(|e| format!("count repository_activity failed: {e}"))?;

            let q = match (push_type, since) {
                (Some(_), Some(_)) => format!(
                    "{SELECT_PG}
 WHERE a.repository_id = $1 AND a.push_type = $2 AND a.created_at >= $3::timestamptz
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT $4 OFFSET $5"
                ),
                (Some(_), None) => format!(
                    "{SELECT_PG}
 WHERE a.repository_id = $1 AND a.push_type = $2
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT $3 OFFSET $4"
                ),
                (None, Some(_)) => format!(
                    "{SELECT_PG}
 WHERE a.repository_id = $1 AND a.created_at >= $2::timestamptz
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT $3 OFFSET $4"
                ),
                (None, None) => format!(
                    "{SELECT_PG}
 WHERE a.repository_id = $1
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT $2 OFFSET $3"
                ),
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(&*q)).bind(repository_id);
            if let Some(pt) = push_type {
                query = query.bind(pt);
            }
            if let Some(s) = since {
                query = query.bind(s);
            }
            query = query.bind(limit).bind(offset);
            let rows = query
                .fetch_all(p)
                .await
                .map_err(|e| format!("list repository_activity failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_activity!(&r));
            }
            Ok((out, total))
        }
        DbPool::MySql(p) => {
            let total: i64 = match (push_type, since) {
                (Some(pt), Some(s)) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = ? AND push_type = ? AND created_at >= ?",
                    )
                    .bind(repository_id)
                    .bind(pt)
                    .bind(s)
                    .fetch_one(p)
                    .await
                }
                (Some(pt), None) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = ? AND push_type = ?",
                    )
                    .bind(repository_id)
                    .bind(pt)
                    .fetch_one(p)
                    .await
                }
                (None, Some(s)) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = ? AND created_at >= ?",
                    )
                    .bind(repository_id)
                    .bind(s)
                    .fetch_one(p)
                    .await
                }
                (None, None) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity WHERE repository_id = ?",
                    )
                    .bind(repository_id)
                    .fetch_one(p)
                    .await
                }
            }
            .map_err(|e| format!("count repository_activity failed: {e}"))?;

            let q = match (push_type, since) {
                (Some(_), Some(_)) => format!(
                    "{SELECT_MYSQL}
 WHERE a.repository_id = ? AND a.push_type = ? AND a.created_at >= ?
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ? OFFSET ?"
                ),
                (Some(_), None) => format!(
                    "{SELECT_MYSQL}
 WHERE a.repository_id = ? AND a.push_type = ?
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ? OFFSET ?"
                ),
                (None, Some(_)) => format!(
                    "{SELECT_MYSQL}
 WHERE a.repository_id = ? AND a.created_at >= ?
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ? OFFSET ?"
                ),
                (None, None) => format!(
                    "{SELECT_MYSQL}
 WHERE a.repository_id = ?
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ? OFFSET ?"
                ),
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(&*q)).bind(repository_id);
            if let Some(pt) = push_type {
                query = query.bind(pt);
            }
            if let Some(s) = since {
                query = query.bind(s);
            }
            query = query.bind(limit).bind(offset);
            let rows = query
                .fetch_all(p)
                .await
                .map_err(|e| format!("list repository_activity failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_activity!(&r));
            }
            Ok((out, total))
        }
        DbPool::Sqlite(p) => {
            let total: i64 = match (push_type, since) {
                (Some(pt), Some(s)) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = ?1 AND push_type = ?2 AND created_at >= ?3",
                    )
                    .bind(repository_id)
                    .bind(pt)
                    .bind(s)
                    .fetch_one(p)
                    .await
                }
                (Some(pt), None) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = ?1 AND push_type = ?2",
                    )
                    .bind(repository_id)
                    .bind(pt)
                    .fetch_one(p)
                    .await
                }
                (None, Some(s)) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity
                     WHERE repository_id = ?1 AND created_at >= ?2",
                    )
                    .bind(repository_id)
                    .bind(s)
                    .fetch_one(p)
                    .await
                }
                (None, None) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM repository_activity WHERE repository_id = ?1",
                    )
                    .bind(repository_id)
                    .fetch_one(p)
                    .await
                }
            }
            .map_err(|e| format!("count repository_activity failed: {e}"))?;

            let q = match (push_type, since) {
                (Some(_), Some(_)) => format!(
                    "{SELECT_SQLITE}
 WHERE a.repository_id = ?1 AND a.push_type = ?2 AND a.created_at >= ?3
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ?4 OFFSET ?5"
                ),
                (Some(_), None) => format!(
                    "{SELECT_SQLITE}
 WHERE a.repository_id = ?1 AND a.push_type = ?2
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ?3 OFFSET ?4"
                ),
                (None, Some(_)) => format!(
                    "{SELECT_SQLITE}
 WHERE a.repository_id = ?1 AND a.created_at >= ?2
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ?3 OFFSET ?4"
                ),
                (None, None) => format!(
                    "{SELECT_SQLITE}
 WHERE a.repository_id = ?1
 ORDER BY a.created_at DESC, a.id DESC
 LIMIT ?2 OFFSET ?3"
                ),
            };
            let mut query = sqlx::query(sqlx::AssertSqlSafe(&*q)).bind(repository_id);
            if let Some(pt) = push_type {
                query = query.bind(pt);
            }
            if let Some(s) = since {
                query = query.bind(s);
            }
            query = query.bind(limit).bind(offset);
            let rows = query
                .fetch_all(p)
                .await
                .map_err(|e| format!("list repository_activity failed: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                out.push(map_activity!(&r));
            }
            Ok((out, total))
        }
    }
}
