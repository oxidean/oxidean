//! Repository stars + fork network helpers (Phase 21 / D-SOC-02, D-SOC-14).

use sqlx::Row;

use crate::pool::DbPool;
use crate::repositories::{self, RepositoryRow};

/// Idempotent star: insert membership and bump counter when newly inserted.
pub async fn star_repository(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("star begin: {e}"))?;
            let inserted = sqlx::query(
                "INSERT INTO repository_stars (user_id, repository_id)
                 VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(user_id)
            .bind(repository_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("star insert: {e}"))?
            .rows_affected();
            if inserted > 0 {
                sqlx::query(
                    "UPDATE repositories SET star_count = star_count + 1, updated_at = now()
                     WHERE id = $1 AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("star bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT star_count FROM repositories WHERE id = $1",
            )
            .bind(repository_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("star count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("star commit: {e}"))?;
            Ok(count)
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("star begin: {e}"))?;
            let inserted = sqlx::query(
                "INSERT IGNORE INTO repository_stars (user_id, repository_id) VALUES (?, ?)",
            )
            .bind(user_id)
            .bind(repository_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("star insert: {e}"))?
            .rows_affected();
            if inserted > 0 {
                sqlx::query(
                    "UPDATE repositories SET star_count = star_count + 1, updated_at = NOW()
                     WHERE id = ? AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("star bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar("SELECT star_count FROM repositories WHERE id = ?")
                .bind(repository_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("star count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("star commit: {e}"))?;
            Ok(count)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("star begin: {e}"))?;
            let inserted = sqlx::query(
                "INSERT OR IGNORE INTO repository_stars (user_id, repository_id)
                 VALUES (?1, ?2)",
            )
            .bind(user_id)
            .bind(repository_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("star insert: {e}"))?
            .rows_affected();
            if inserted > 0 {
                sqlx::query(
                    "UPDATE repositories SET star_count = star_count + 1,
                     updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                     WHERE id = ?1 AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("star bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT star_count FROM repositories WHERE id = ?1",
            )
            .bind(repository_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("star count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("star commit: {e}"))?;
            Ok(count)
        }
    }
}

/// Idempotent unstar: delete membership and decrement counter when a row was removed.
pub async fn unstar_repository(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("unstar begin: {e}"))?;
            let deleted = sqlx::query(
                "DELETE FROM repository_stars WHERE user_id = $1 AND repository_id = $2",
            )
            .bind(user_id)
            .bind(repository_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("unstar delete: {e}"))?
            .rows_affected();
            if deleted > 0 {
                sqlx::query(
                    "UPDATE repositories SET star_count = GREATEST(star_count - 1, 0),
                     updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unstar bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT star_count FROM repositories WHERE id = $1",
            )
            .bind(repository_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("unstar count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("unstar commit: {e}"))?;
            Ok(count)
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("unstar begin: {e}"))?;
            let deleted = sqlx::query(
                "DELETE FROM repository_stars WHERE user_id = ? AND repository_id = ?",
            )
            .bind(user_id)
            .bind(repository_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("unstar delete: {e}"))?
            .rows_affected();
            if deleted > 0 {
                sqlx::query(
                    "UPDATE repositories SET star_count = GREATEST(star_count - 1, 0),
                     updated_at = NOW() WHERE id = ? AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unstar bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar("SELECT star_count FROM repositories WHERE id = ?")
                .bind(repository_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("unstar count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("unstar commit: {e}"))?;
            Ok(count)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("unstar begin: {e}"))?;
            let deleted = sqlx::query(
                "DELETE FROM repository_stars WHERE user_id = ?1 AND repository_id = ?2",
            )
            .bind(user_id)
            .bind(repository_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("unstar delete: {e}"))?
            .rows_affected();
            if deleted > 0 {
                sqlx::query(
                    "UPDATE repositories SET star_count = MAX(star_count - 1, 0),
                     updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                     WHERE id = ?1 AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unstar bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT star_count FROM repositories WHERE id = ?1",
            )
            .bind(repository_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("unstar count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("unstar commit: {e}"))?;
            Ok(count)
        }
    }
}

pub async fn get_star_count(pool: &DbPool, repository_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COALESCE(star_count, 0) FROM repositories WHERE id = $1",
        )
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get star_count: {e}"))
        .map(|o| o.unwrap_or(0)),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COALESCE(star_count, 0) FROM repositories WHERE id = ?",
        )
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get star_count: {e}"))
        .map(|o| o.unwrap_or(0)),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COALESCE(star_count, 0) FROM repositories WHERE id = ?1",
        )
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get star_count: {e}"))
        .map(|o| o.unwrap_or(0)),
    }
}

pub async fn has_starred(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
) -> Result<bool, String> {
    let count: i64 = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repository_stars WHERE user_id = $1 AND repository_id = $2",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("has_starred: {e}"))?,
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repository_stars WHERE user_id = ? AND repository_id = ?",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("has_starred: {e}"))?,
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repository_stars WHERE user_id = ?1 AND repository_id = ?2",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("has_starred: {e}"))?,
    };
    Ok(count > 0)
}

pub async fn get_fork_network_id(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Option<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT fork_network_id FROM repositories WHERE id = $1",
        )
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get fork_network_id: {e}")),
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT fork_network_id FROM repositories WHERE id = ?")
                .bind(repository_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get fork_network_id: {e}"))
        }
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT fork_network_id FROM repositories WHERE id = ?1",
        )
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get fork_network_id: {e}")),
    }
}

/// Set fork_network_id (roots: own id; forks: source network root).
pub async fn set_fork_network_id(
    pool: &DbPool,
    repository_id: &str,
    network_id: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE repositories SET fork_network_id = $2 WHERE id = $1")
                .bind(repository_id)
                .bind(network_id)
                .execute(p)
                .await
                .map_err(|e| format!("set fork_network_id: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE repositories SET fork_network_id = ? WHERE id = ?")
                .bind(network_id)
                .bind(repository_id)
                .execute(p)
                .await
                .map_err(|e| format!("set fork_network_id: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE repositories SET fork_network_id = ?2 WHERE id = ?1")
                .bind(repository_id)
                .bind(network_id)
                .execute(p)
                .await
                .map_err(|e| format!("set fork_network_id: {e}"))?;
        }
    }
    Ok(())
}

/// Active fork for (owner_id, fork_network_id), if any.
pub async fn find_active_fork_in_network(
    pool: &DbPool,
    owner_id: &str,
    fork_network_id: &str,
) -> Result<Option<RepositoryRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(
                "SELECT id FROM repositories
                 WHERE owner_id = $1 AND fork_network_id = $2
                   AND forked_from_repo_id IS NOT NULL AND deleted_at IS NULL
                 LIMIT 1",
            )
            .bind(owner_id)
            .bind(fork_network_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find fork in network: {e}"))?;
            match row {
                Some(r) => {
                    let id: String = r.try_get("id").map_err(|e| format!("row: {e}"))?;
                    repositories::find_by_id(pool, &id).await
                }
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(
                "SELECT id FROM repositories
                 WHERE owner_id = ? AND fork_network_id = ?
                   AND forked_from_repo_id IS NOT NULL AND deleted_at IS NULL
                 LIMIT 1",
            )
            .bind(owner_id)
            .bind(fork_network_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find fork in network: {e}"))?;
            match row {
                Some(r) => {
                    let id: String = r.try_get("id").map_err(|e| format!("row: {e}"))?;
                    repositories::find_by_id(pool, &id).await
                }
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(
                "SELECT id FROM repositories
                 WHERE owner_id = ?1 AND fork_network_id = ?2
                   AND forked_from_repo_id IS NOT NULL AND deleted_at IS NULL
                 LIMIT 1",
            )
            .bind(owner_id)
            .bind(fork_network_id)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find fork in network: {e}"))?;
            match row {
                Some(r) => {
                    let id: String = r.try_get("id").map_err(|e| format!("row: {e}"))?;
                    repositories::find_by_id(pool, &id).await
                }
                None => Ok(None),
            }
        }
    }
}

/// Starred repo ids for a user, newest-starred first, with offset/limit.
pub async fn list_starred_repo_ids(
    pool: &DbPool,
    user_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT repository_id FROM repository_stars
             WHERE user_id = $1
             ORDER BY created_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list starred: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT repository_id FROM repository_stars
             WHERE user_id = ?
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list starred: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT repository_id FROM repository_stars
             WHERE user_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list starred: {e}")),
    }
}

/// Public explore listing: star_count DESC, updated_at DESC.
pub async fn list_explore(
    pool: &DbPool,
    q: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<RepositoryRow>, String> {
    let q_pat = q.map(|s| {
        let escaped = s.replace('%', r"\%").replace('_', r"\_");
        format!("%{escaped}%")
    });
    match pool {
        DbPool::Postgres(p) => {
            let rows = if let Some(ref pat) = q_pat {
                sqlx::query(
                    "SELECT id FROM repositories
                     WHERE deleted_at IS NULL AND lower(visibility) = 'public'
                       AND (name ILIKE $1 ESCAPE '\\' OR COALESCE(description, '') ILIKE $1 ESCAPE '\\')
                     ORDER BY star_count DESC, updated_at DESC
                     LIMIT $2 OFFSET $3",
                )
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(
                    "SELECT id FROM repositories
                     WHERE deleted_at IS NULL AND lower(visibility) = 'public'
                     ORDER BY star_count DESC, updated_at DESC
                     LIMIT $1 OFFSET $2",
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("explore: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let id: String = r.try_get("id").map_err(|e| format!("row: {e}"))?;
                if let Some(row) = repositories::find_by_id(pool, &id).await? {
                    out.push(row);
                }
            }
            Ok(out)
        }
        DbPool::MySql(p) => {
            let rows = if let Some(ref pat) = q_pat {
                sqlx::query(
                    "SELECT id FROM repositories
                     WHERE deleted_at IS NULL AND LOWER(visibility) = 'public'
                       AND (name LIKE ? OR COALESCE(description, '') LIKE ?)
                     ORDER BY star_count DESC, updated_at DESC
                     LIMIT ? OFFSET ?",
                )
                .bind(pat)
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(
                    "SELECT id FROM repositories
                     WHERE deleted_at IS NULL AND LOWER(visibility) = 'public'
                     ORDER BY star_count DESC, updated_at DESC
                     LIMIT ? OFFSET ?",
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("explore: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let id: String = r.try_get("id").map_err(|e| format!("row: {e}"))?;
                if let Some(row) = repositories::find_by_id(pool, &id).await? {
                    out.push(row);
                }
            }
            Ok(out)
        }
        DbPool::Sqlite(p) => {
            let rows = if let Some(ref pat) = q_pat {
                sqlx::query(
                    "SELECT id FROM repositories
                     WHERE deleted_at IS NULL AND lower(visibility) = 'public'
                       AND (name LIKE ?1 ESCAPE '\\' OR COALESCE(description, '') LIKE ?1 ESCAPE '\\')
                     ORDER BY star_count DESC, updated_at DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(
                    "SELECT id FROM repositories
                     WHERE deleted_at IS NULL AND lower(visibility) = 'public'
                     ORDER BY star_count DESC, updated_at DESC
                     LIMIT ?1 OFFSET ?2",
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("explore: {e}"))?;
            let mut out = Vec::with_capacity(rows.len());
            for r in rows {
                let id: String = r.try_get("id").map_err(|e| format!("row: {e}"))?;
                if let Some(row) = repositories::find_by_id(pool, &id).await? {
                    out.push(row);
                }
            }
            Ok(out)
        }
    }
}

/// Stargazer row for `repo.stargazers.list` (no email).
#[derive(Debug, Clone)]
pub struct RepoStargazerListRow {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub starred_at: String,
}

/// Fork list row for `repo.forks.list`.
#[derive(Debug, Clone)]
pub struct RepoForkListRow {
    pub id: String,
    pub owner_username: String,
    pub name: String,
    pub description: String,
    pub star_count: i64,
    pub fork_count: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Set when owner is a user and has an avatar.
    pub owner_user_id: Option<String>,
    pub has_owner_avatar: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ForkListSort {
    Stars,
    Updated,
    Created,
}

fn social_like_pat(q: &str) -> String {
    let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

macro_rules! map_stargazer {
    ($row:expr) => {{
        let row = $row;
        RepoStargazerListRow {
            user_id: row.try_get("user_id").map_err(|e| format!("stargazer row: {e}"))?,
            username: row.try_get("username").map_err(|e| format!("stargazer row: {e}"))?,
            display_name: row
                .try_get("display_name")
                .map_err(|e| format!("stargazer row: {e}"))?,
            avatar_path: row
                .try_get("avatar_path")
                .map_err(|e| format!("stargazer row: {e}"))?,
            starred_at: row
                .try_get("starred_at")
                .map_err(|e| format!("stargazer row: {e}"))?,
        }
    }};
}

macro_rules! map_fork_list {
    ($row:expr) => {{
        let row = $row;
        let owner_avatar_path: Option<String> = row
            .try_get("owner_avatar_path")
            .map_err(|e| format!("fork row: {e}"))?;
        let owner_user_id: Option<String> = row
            .try_get("owner_user_id")
            .map_err(|e| format!("fork row: {e}"))?;
        RepoForkListRow {
            id: row.try_get("id").map_err(|e| format!("fork row: {e}"))?,
            owner_username: row
                .try_get("owner_username")
                .map_err(|e| format!("fork row: {e}"))?,
            name: row.try_get("name").map_err(|e| format!("fork row: {e}"))?,
            description: row
                .try_get("description")
                .map_err(|e| format!("fork row: {e}"))?,
            star_count: row
                .try_get("star_count")
                .map_err(|e| format!("fork row: {e}"))?,
            fork_count: row
                .try_get("fork_count")
                .map_err(|e| format!("fork row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("fork row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("fork row: {e}"))?,
            has_owner_avatar: owner_avatar_path.is_some(),
            owner_user_id,
        }
    }};
}

/// Paginated stargazers, newest first. Optional `q` filters username/display_name.
pub async fn list_repo_stargazers(
    pool: &DbPool,
    repository_id: &str,
    q: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<RepoStargazerListRow>, String> {
    let pat = q.filter(|s| !s.trim().is_empty()).map(|s| social_like_pat(s.trim()));
    match pool {
        DbPool::Postgres(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(
                    "SELECT s.user_id, u.username, u.display_name, u.avatar_path,
                            to_char(s.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS starred_at
                     FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = $1
                       AND (u.username ILIKE $2 ESCAPE '\\' OR COALESCE(u.display_name, '') ILIKE $2 ESCAPE '\\')
                     ORDER BY s.created_at DESC
                     LIMIT $3 OFFSET $4",
                )
                .bind(repository_id)
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(
                    "SELECT s.user_id, u.username, u.display_name, u.avatar_path,
                            to_char(s.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS starred_at
                     FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = $1
                     ORDER BY s.created_at DESC
                     LIMIT $2 OFFSET $3",
                )
                .bind(repository_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list stargazers: {e}"))?;
            rows.into_iter().map(|r| Ok(map_stargazer!(&r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(
                    "SELECT s.user_id, u.username, u.display_name, u.avatar_path,
                            DATE_FORMAT(s.created_at, '%Y-%m-%dT%H:%i:%sZ') AS starred_at
                     FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = ?
                       AND (u.username LIKE ? ESCAPE '\\\\' OR COALESCE(u.display_name, '') LIKE ? ESCAPE '\\\\')
                     ORDER BY s.created_at DESC
                     LIMIT ? OFFSET ?",
                )
                .bind(repository_id)
                .bind(pat)
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(
                    "SELECT s.user_id, u.username, u.display_name, u.avatar_path,
                            DATE_FORMAT(s.created_at, '%Y-%m-%dT%H:%i:%sZ') AS starred_at
                     FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = ?
                     ORDER BY s.created_at DESC
                     LIMIT ? OFFSET ?",
                )
                .bind(repository_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list stargazers: {e}"))?;
            rows.into_iter().map(|r| Ok(map_stargazer!(&r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(
                    "SELECT s.user_id, u.username, u.display_name, u.avatar_path,
                            strftime('%Y-%m-%dT%H:%M:%SZ', s.created_at) AS starred_at
                     FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = ?1
                       AND (u.username LIKE ?2 ESCAPE '\\' OR COALESCE(u.display_name, '') LIKE ?2 ESCAPE '\\')
                     ORDER BY s.created_at DESC
                     LIMIT ?3 OFFSET ?4",
                )
                .bind(repository_id)
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(
                    "SELECT s.user_id, u.username, u.display_name, u.avatar_path,
                            strftime('%Y-%m-%dT%H:%M:%SZ', s.created_at) AS starred_at
                     FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = ?1
                     ORDER BY s.created_at DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .bind(repository_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list stargazers: {e}"))?;
            rows.into_iter().map(|r| Ok(map_stargazer!(&r))).collect()
        }
    }
}

pub async fn count_repo_stargazers(
    pool: &DbPool,
    repository_id: &str,
    q: Option<&str>,
) -> Result<i64, String> {
    let pat = q.filter(|s| !s.trim().is_empty()).map(|s| social_like_pat(s.trim()));
    match pool {
        DbPool::Postgres(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = $1
                       AND (u.username ILIKE $2 ESCAPE '\\' OR COALESCE(u.display_name, '') ILIKE $2 ESCAPE '\\')",
                )
                .bind(repository_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_stars WHERE repository_id = $1",
                )
                .bind(repository_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count stargazers: {e}"))
        }
        DbPool::MySql(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = ?
                       AND (u.username LIKE ? ESCAPE '\\\\' OR COALESCE(u.display_name, '') LIKE ? ESCAPE '\\\\')",
                )
                .bind(repository_id)
                .bind(pat)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_stars WHERE repository_id = ?",
                )
                .bind(repository_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count stargazers: {e}"))
        }
        DbPool::Sqlite(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_stars s
                     JOIN users u ON u.id = s.user_id
                     WHERE s.repository_id = ?1
                       AND (u.username LIKE ?2 ESCAPE '\\' OR COALESCE(u.display_name, '') LIKE ?2 ESCAPE '\\')",
                )
                .bind(repository_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_stars WHERE repository_id = ?1",
                )
                .bind(repository_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count stargazers: {e}"))
        }
    }
}

fn fork_order_sql(sort: ForkListSort) -> &'static str {
    match sort {
        ForkListSort::Stars => "COALESCE(r.star_count, 0) DESC, r.updated_at DESC",
        ForkListSort::Updated => "r.updated_at DESC",
        ForkListSort::Created => "r.created_at DESC",
    }
}

/// Active forks in a network (excludes the root when it has no forked_from).
pub async fn list_network_forks(
    pool: &DbPool,
    fork_network_id: &str,
    q: Option<&str>,
    sort: ForkListSort,
    offset: i64,
    limit: i64,
) -> Result<Vec<RepoForkListRow>, String> {
    let pat = q.filter(|s| !s.trim().is_empty()).map(|s| social_like_pat(s.trim()));
    let order = fork_order_sql(sort);
    match pool {
        DbPool::Postgres(p) => {
            let sql = if pat.is_some() {
                format!(
                    "SELECT r.id,
                            CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
                            r.name, r.description,
                            COALESCE(r.star_count, 0) AS star_count,
                            COALESCE(r.fork_count, 0) AS fork_count,
                            to_char(r.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
                            to_char(r.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at,
                            CASE WHEN r.owner_type = 'user' THEN u.id ELSE NULL END AS owner_user_id,
                            CASE WHEN r.owner_type = 'user' THEN u.avatar_path ELSE NULL END AS owner_avatar_path
                     FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = $1
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                       AND (
                         CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END ILIKE $2 ESCAPE '\\'
                         OR r.name ILIKE $2 ESCAPE '\\'
                       )
                     ORDER BY {order}
                     LIMIT $3 OFFSET $4"
                )
            } else {
                format!(
                    "SELECT r.id,
                            CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
                            r.name, r.description,
                            COALESCE(r.star_count, 0) AS star_count,
                            COALESCE(r.fork_count, 0) AS fork_count,
                            to_char(r.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at,
                            to_char(r.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS updated_at,
                            CASE WHEN r.owner_type = 'user' THEN u.id ELSE NULL END AS owner_user_id,
                            CASE WHEN r.owner_type = 'user' THEN u.avatar_path ELSE NULL END AS owner_avatar_path
                     FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = $1
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                     ORDER BY {order}
                     LIMIT $2 OFFSET $3"
                )
            };
            let rows = if let Some(ref pat) = pat {
                sqlx::query(&sql)
                    .bind(fork_network_id)
                    .bind(pat)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(p)
                    .await
            } else {
                sqlx::query(&sql)
                    .bind(fork_network_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(p)
                    .await
            }
            .map_err(|e| format!("list forks: {e}"))?;
            rows.into_iter().map(|r| Ok(map_fork_list!(&r))).collect()
        }
        DbPool::MySql(p) => {
            let sql = if pat.is_some() {
                format!(
                    "SELECT r.id,
                            CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
                            r.name, r.description,
                            COALESCE(r.star_count, 0) AS star_count,
                            COALESCE(r.fork_count, 0) AS fork_count,
                            DATE_FORMAT(r.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                            DATE_FORMAT(r.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at,
                            CASE WHEN r.owner_type = 'user' THEN u.id ELSE NULL END AS owner_user_id,
                            CASE WHEN r.owner_type = 'user' THEN u.avatar_path ELSE NULL END AS owner_avatar_path
                     FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = ?
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                       AND (
                         CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END LIKE ? ESCAPE '\\\\'
                         OR r.name LIKE ? ESCAPE '\\\\'
                       )
                     ORDER BY {order}
                     LIMIT ? OFFSET ?"
                )
            } else {
                format!(
                    "SELECT r.id,
                            CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
                            r.name, r.description,
                            COALESCE(r.star_count, 0) AS star_count,
                            COALESCE(r.fork_count, 0) AS fork_count,
                            DATE_FORMAT(r.created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at,
                            DATE_FORMAT(r.updated_at, '%Y-%m-%dT%H:%i:%sZ') AS updated_at,
                            CASE WHEN r.owner_type = 'user' THEN u.id ELSE NULL END AS owner_user_id,
                            CASE WHEN r.owner_type = 'user' THEN u.avatar_path ELSE NULL END AS owner_avatar_path
                     FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = ?
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                     ORDER BY {order}
                     LIMIT ? OFFSET ?"
                )
            };
            let rows = if let Some(ref pat) = pat {
                sqlx::query(&sql)
                    .bind(fork_network_id)
                    .bind(pat)
                    .bind(pat)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(p)
                    .await
            } else {
                sqlx::query(&sql)
                    .bind(fork_network_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(p)
                    .await
            }
            .map_err(|e| format!("list forks: {e}"))?;
            rows.into_iter().map(|r| Ok(map_fork_list!(&r))).collect()
        }
        DbPool::Sqlite(p) => {
            let sql = if pat.is_some() {
                format!(
                    "SELECT r.id,
                            CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
                            r.name, r.description,
                            COALESCE(r.star_count, 0) AS star_count,
                            COALESCE(r.fork_count, 0) AS fork_count,
                            strftime('%Y-%m-%dT%H:%M:%SZ', r.created_at) AS created_at,
                            strftime('%Y-%m-%dT%H:%M:%SZ', r.updated_at) AS updated_at,
                            CASE WHEN r.owner_type = 'user' THEN u.id ELSE NULL END AS owner_user_id,
                            CASE WHEN r.owner_type = 'user' THEN u.avatar_path ELSE NULL END AS owner_avatar_path
                     FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = ?1
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                       AND (
                         CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END LIKE ?2 ESCAPE '\\'
                         OR r.name LIKE ?2 ESCAPE '\\'
                       )
                     ORDER BY {order}
                     LIMIT ?3 OFFSET ?4"
                )
            } else {
                format!(
                    "SELECT r.id,
                            CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
                            r.name, r.description,
                            COALESCE(r.star_count, 0) AS star_count,
                            COALESCE(r.fork_count, 0) AS fork_count,
                            strftime('%Y-%m-%dT%H:%M:%SZ', r.created_at) AS created_at,
                            strftime('%Y-%m-%dT%H:%M:%SZ', r.updated_at) AS updated_at,
                            CASE WHEN r.owner_type = 'user' THEN u.id ELSE NULL END AS owner_user_id,
                            CASE WHEN r.owner_type = 'user' THEN u.avatar_path ELSE NULL END AS owner_avatar_path
                     FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = ?1
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                     ORDER BY {order}
                     LIMIT ?2 OFFSET ?3"
                )
            };
            let rows = if let Some(ref pat) = pat {
                sqlx::query(&sql)
                    .bind(fork_network_id)
                    .bind(pat)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(p)
                    .await
            } else {
                sqlx::query(&sql)
                    .bind(fork_network_id)
                    .bind(limit)
                    .bind(offset)
                    .fetch_all(p)
                    .await
            }
            .map_err(|e| format!("list forks: {e}"))?;
            rows.into_iter().map(|r| Ok(map_fork_list!(&r))).collect()
        }
    }
}

/// Network member row for `repo.insights.forkNetwork` (GIT-26) — includes the
/// network root and each repo's immediate parent (`forked_from`).
#[derive(Debug, Clone)]
pub struct ForkNetworkRow {
    pub id: String,
    pub owner_username: String,
    pub name: String,
    /// `forked_from_repo_id`; `None` on the network root.
    pub parent_repo_id: Option<String>,
    /// Parent repo owner slug when the parent row still exists.
    pub parent_owner: Option<String>,
    /// Parent repo name when the parent row still exists.
    pub parent_name: Option<String>,
    pub star_count: i64,
    pub fork_count: i64,
    pub created_at: String,
    pub updated_at: String,
    /// Set when owner is a user and has an avatar.
    pub owner_user_id: Option<String>,
    pub has_owner_avatar: bool,
}

macro_rules! map_fork_network {
    ($row:expr) => {{
        let row = $row;
        let owner_avatar_path: Option<String> = row
            .try_get("owner_avatar_path")
            .map_err(|e| format!("fork network row: {e}"))?;
        ForkNetworkRow {
            id: row.try_get("id").map_err(|e| format!("fork network row: {e}"))?,
            owner_username: row
                .try_get("owner_username")
                .map_err(|e| format!("fork network row: {e}"))?,
            name: row
                .try_get("name")
                .map_err(|e| format!("fork network row: {e}"))?,
            parent_repo_id: row
                .try_get("parent_repo_id")
                .map_err(|e| format!("fork network row: {e}"))?,
            parent_owner: row
                .try_get("parent_owner")
                .map_err(|e| format!("fork network row: {e}"))?,
            parent_name: row
                .try_get("parent_name")
                .map_err(|e| format!("fork network row: {e}"))?,
            star_count: row
                .try_get("star_count")
                .map_err(|e| format!("fork network row: {e}"))?,
            fork_count: row
                .try_get("fork_count")
                .map_err(|e| format!("fork network row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("fork network row: {e}"))?,
            updated_at: row
                .try_get("updated_at")
                .map_err(|e| format!("fork network row: {e}"))?,
            owner_user_id: row
                .try_get("owner_user_id")
                .map_err(|e| format!("fork network row: {e}"))?,
            has_owner_avatar: owner_avatar_path.is_some(),
        }
    }};
}

// Member select shared by `list_fork_network` / `count_fork_network_members`.
// Includes the network root (forked_from IS NULL) and the queried repo even
// when private — the caller already resolved Read on it, so listing it leaks
// nothing the repo.get response does not already expose.
const FORK_NETWORK_SELECT: &str = "
    SELECT r.id,
           CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END AS owner_username,
           r.name,
           r.forked_from_repo_id AS parent_repo_id,
           CASE WHEN p.owner_type = 'org' THEN po.slug ELSE pu.username END AS parent_owner,
           p.name AS parent_name,
           COALESCE(r.star_count, 0) AS star_count,
           COALESCE(r.fork_count, 0) AS fork_count,
           {created_at} AS created_at,
           {updated_at} AS updated_at,
           CASE WHEN r.owner_type = 'user' THEN u.id ELSE NULL END AS owner_user_id,
           CASE WHEN r.owner_type = 'user' THEN u.avatar_path ELSE NULL END AS owner_avatar_path
    FROM repositories r
    LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
    LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
    LEFT JOIN repositories p ON p.id = r.forked_from_repo_id
    LEFT JOIN users pu ON p.owner_type = 'user' AND pu.id = p.owner_id
    LEFT JOIN organizations po ON p.owner_type = 'org' AND po.id = p.owner_id";

/// Public members of a fork network (root included), oldest first — plus
/// `current_repo_id` regardless of visibility. Capped by `limit`.
pub async fn list_fork_network(
    pool: &DbPool,
    fork_network_id: &str,
    current_repo_id: &str,
    limit: i64,
) -> Result<Vec<ForkNetworkRow>, String> {
    let limit = limit.clamp(1, 500);
    match pool {
        DbPool::Postgres(p) => {
            let sql = FORK_NETWORK_SELECT
                .replace("{created_at}", "to_char(r.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')")
                .replace("{updated_at}", "to_char(r.updated_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"')")
                + "
    WHERE r.fork_network_id = $1
      AND r.deleted_at IS NULL
      AND (lower(r.visibility) = 'public' OR r.id = $2)
    ORDER BY r.created_at ASC, r.id ASC
    LIMIT $3";
            let rows = sqlx::query(&sql)
                .bind(fork_network_id)
                .bind(current_repo_id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list fork network: {e}"))?;
            rows.into_iter().map(|r| Ok(map_fork_network!(&r))).collect()
        }
        DbPool::MySql(p) => {
            let sql = FORK_NETWORK_SELECT
                .replace("{created_at}", "DATE_FORMAT(r.created_at, '%Y-%m-%dT%H:%i:%sZ')")
                .replace("{updated_at}", "DATE_FORMAT(r.updated_at, '%Y-%m-%dT%H:%i:%sZ')")
                + "
    WHERE r.fork_network_id = ?
      AND r.deleted_at IS NULL
      AND (lower(r.visibility) = 'public' OR r.id = ?)
    ORDER BY r.created_at ASC, r.id ASC
    LIMIT ?";
            let rows = sqlx::query(&sql)
                .bind(fork_network_id)
                .bind(current_repo_id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list fork network: {e}"))?;
            rows.into_iter().map(|r| Ok(map_fork_network!(&r))).collect()
        }
        DbPool::Sqlite(p) => {
            let sql = FORK_NETWORK_SELECT
                .replace("{created_at}", "strftime('%Y-%m-%dT%H:%M:%SZ', r.created_at)")
                .replace("{updated_at}", "strftime('%Y-%m-%dT%H:%M:%SZ', r.updated_at)")
                + "
    WHERE r.fork_network_id = ?1
      AND r.deleted_at IS NULL
      AND (lower(r.visibility) = 'public' OR r.id = ?2)
    ORDER BY r.created_at ASC, r.id ASC
    LIMIT ?3";
            let rows = sqlx::query(&sql)
                .bind(fork_network_id)
                .bind(current_repo_id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list fork network: {e}"))?;
            rows.into_iter().map(|r| Ok(map_fork_network!(&r))).collect()
        }
    }
}

/// Total rows `list_fork_network` can return for the same filter (for the
/// `truncated` flag; a COUNT over the same visibility window).
pub async fn count_fork_network_members(
    pool: &DbPool,
    fork_network_id: &str,
    current_repo_id: &str,
) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repositories r
             WHERE r.fork_network_id = $1
               AND r.deleted_at IS NULL
               AND (lower(r.visibility) = 'public' OR r.id = $2)",
        )
        .bind(fork_network_id)
        .bind(current_repo_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count fork network: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repositories r
             WHERE r.fork_network_id = ?
               AND r.deleted_at IS NULL
               AND (lower(r.visibility) = 'public' OR r.id = ?)",
        )
        .bind(fork_network_id)
        .bind(current_repo_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count fork network: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repositories r
             WHERE r.fork_network_id = ?1
               AND r.deleted_at IS NULL
               AND (lower(r.visibility) = 'public' OR r.id = ?2)",
        )
        .bind(fork_network_id)
        .bind(current_repo_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("count fork network: {e}")),
    }
}

pub async fn count_network_forks(
    pool: &DbPool,
    fork_network_id: &str,
    q: Option<&str>,
) -> Result<i64, String> {
    let pat = q.filter(|s| !s.trim().is_empty()).map(|s| social_like_pat(s.trim()));
    match pool {
        DbPool::Postgres(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = $1
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                       AND (
                         CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END ILIKE $2 ESCAPE '\\'
                         OR r.name ILIKE $2 ESCAPE '\\'
                       )",
                )
                .bind(fork_network_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repositories
                     WHERE fork_network_id = $1
                       AND forked_from_repo_id IS NOT NULL
                       AND deleted_at IS NULL
                       AND lower(visibility) = 'public'",
                )
                .bind(fork_network_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count forks: {e}"))
        }
        DbPool::MySql(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = ?
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                       AND (
                         CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END LIKE ? ESCAPE '\\\\'
                         OR r.name LIKE ? ESCAPE '\\\\'
                       )",
                )
                .bind(fork_network_id)
                .bind(pat)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repositories
                     WHERE fork_network_id = ?
                       AND forked_from_repo_id IS NOT NULL
                       AND deleted_at IS NULL
                       AND lower(visibility) = 'public'",
                )
                .bind(fork_network_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count forks: {e}"))
        }
        DbPool::Sqlite(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repositories r
                     LEFT JOIN users u ON r.owner_type = 'user' AND u.id = r.owner_id
                     LEFT JOIN organizations o ON r.owner_type = 'org' AND o.id = r.owner_id
                     WHERE r.fork_network_id = ?1
                       AND r.forked_from_repo_id IS NOT NULL
                       AND r.deleted_at IS NULL
                       AND lower(r.visibility) = 'public'
                       AND (
                         CASE WHEN r.owner_type = 'org' THEN o.slug ELSE u.username END LIKE ?2 ESCAPE '\\'
                         OR r.name LIKE ?2 ESCAPE '\\'
                       )",
                )
                .bind(fork_network_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repositories
                     WHERE fork_network_id = ?1
                       AND forked_from_repo_id IS NOT NULL
                       AND deleted_at IS NULL
                       AND lower(visibility) = 'public'",
                )
                .bind(fork_network_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count forks: {e}"))
        }
    }
}
