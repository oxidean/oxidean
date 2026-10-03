//! User follows — asymmetric follow graph, mirrors watches.rs (DEBT-06).
//! Dialect SQL only.

use crate::pool::DbPool;

/// Idempotent follow: insert the edge when absent. Returns `true` when a new
/// edge was created.
pub async fn follow_user(
    pool: &DbPool,
    follower_id: &str,
    followed_id: &str,
) -> Result<bool, String> {
    let inserted = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO user_follows (follower_id, followed_id)
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(follower_id)
        .bind(followed_id)
        .execute(p)
        .await
        .map_err(|e| format!("follow insert: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "INSERT IGNORE INTO user_follows (follower_id, followed_id) VALUES (?, ?)",
        )
        .bind(follower_id)
        .bind(followed_id)
        .execute(p)
        .await
        .map_err(|e| format!("follow insert: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "INSERT OR IGNORE INTO user_follows (follower_id, followed_id) VALUES (?1, ?2)",
        )
        .bind(follower_id)
        .bind(followed_id)
        .execute(p)
        .await
        .map_err(|e| format!("follow insert: {e}"))?
        .rows_affected(),
    };
    Ok(inserted > 0)
}

/// Idempotent unfollow. Returns `true` when an edge was removed.
pub async fn unfollow_user(
    pool: &DbPool,
    follower_id: &str,
    followed_id: &str,
) -> Result<bool, String> {
    let deleted = match pool {
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM user_follows WHERE follower_id = $1 AND followed_id = $2",
        )
        .bind(follower_id)
        .bind(followed_id)
        .execute(p)
        .await
        .map_err(|e| format!("unfollow delete: {e}"))?
        .rows_affected(),
        DbPool::MySql(p) => sqlx::query(
            "DELETE FROM user_follows WHERE follower_id = ? AND followed_id = ?",
        )
        .bind(follower_id)
        .bind(followed_id)
        .execute(p)
        .await
        .map_err(|e| format!("unfollow delete: {e}"))?
        .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM user_follows WHERE follower_id = ?1 AND followed_id = ?2",
        )
        .bind(follower_id)
        .bind(followed_id)
        .execute(p)
        .await
        .map_err(|e| format!("unfollow delete: {e}"))?
        .rows_affected(),
    };
    Ok(deleted > 0)
}

pub async fn is_following(
    pool: &DbPool,
    follower_id: &str,
    followed_id: &str,
) -> Result<bool, String> {
    let count: i64 = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows WHERE follower_id = $1 AND followed_id = $2",
        )
        .bind(follower_id)
        .bind(followed_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("is_following: {e}"))?,
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows WHERE follower_id = ? AND followed_id = ?",
        )
        .bind(follower_id)
        .bind(followed_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("is_following: {e}"))?,
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows WHERE follower_id = ?1 AND followed_id = ?2",
        )
        .bind(follower_id)
        .bind(followed_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("is_following: {e}"))?,
    };
    Ok(count > 0)
}

/// Accounts following `user_id` (banned users hidden — edges persist for unban).
pub async fn follower_count(pool: &DbPool, user_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows f
             JOIN users u ON u.id = f.follower_id
             WHERE f.followed_id = $1 AND u.banned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("follower_count: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows f
             JOIN users u ON u.id = f.follower_id
             WHERE f.followed_id = ? AND u.banned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("follower_count: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows f
             JOIN users u ON u.id = f.follower_id
             WHERE f.followed_id = ?1 AND u.banned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("follower_count: {e}")),
    }
}

/// Accounts `user_id` follows (banned users hidden — edges persist for unban).
pub async fn following_count(pool: &DbPool, user_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows f
             JOIN users u ON u.id = f.followed_id
             WHERE f.follower_id = $1 AND u.banned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("following_count: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows f
             JOIN users u ON u.id = f.followed_id
             WHERE f.follower_id = ? AND u.banned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("following_count: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_follows f
             JOIN users u ON u.id = f.followed_id
             WHERE f.follower_id = ?1 AND u.banned_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("following_count: {e}")),
    }
}

/// Row for `user.followers.list` / `user.following.list` (no email).
#[derive(Debug, Clone)]
pub struct UserFollowListRow {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub followed_at: String,
}

fn like_pat(q: &str) -> String {
    let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

macro_rules! map_follow {
    ($row:expr) => {{
        let row = $row;
        UserFollowListRow {
            user_id: row.try_get("user_id").map_err(|e| format!("follow row: {e}"))?,
            username: row.try_get("username").map_err(|e| format!("follow row: {e}"))?,
            display_name: row
                .try_get("display_name")
                .map_err(|e| format!("follow row: {e}"))?,
            avatar_path: row
                .try_get("avatar_path")
                .map_err(|e| format!("follow row: {e}"))?,
            followed_at: row
                .try_get("followed_at")
                .map_err(|e| format!("follow row: {e}"))?,
        }
    }};
}

/// `direction` selects the edge side: `followers` lists accounts following
/// `user_id`, `following` lists accounts `user_id` follows.
async fn list_follow_edges(
    pool: &DbPool,
    user_id: &str,
    followers: bool,
    q: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<UserFollowListRow>, String> {
    use sqlx::Row;
    let pat = q
        .filter(|s| !s.trim().is_empty())
        .map(|s| like_pat(s.trim()));
    let (edge_col, join_col) = if followers {
        ("f.followed_id", "f.follower_id")
    } else {
        ("f.follower_id", "f.followed_id")
    };
    match pool {
        DbPool::Postgres(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(&format!(
                    "SELECT u.id AS user_id, u.username, u.display_name, u.avatar_path,
                            to_char(f.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS followed_at
                     FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = $1 AND u.banned_at IS NULL
                       AND (u.username ILIKE $2 ESCAPE '\\' OR COALESCE(u.display_name, '') ILIKE $2 ESCAPE '\\')
                     ORDER BY f.created_at DESC
                     LIMIT $3 OFFSET $4",
                ))
                .bind(user_id)
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(&format!(
                    "SELECT u.id AS user_id, u.username, u.display_name, u.avatar_path,
                            to_char(f.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS followed_at
                     FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = $1 AND u.banned_at IS NULL
                     ORDER BY f.created_at DESC
                     LIMIT $2 OFFSET $3",
                ))
                .bind(user_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list follows: {e}"))?;
            rows.into_iter().map(|r| Ok(map_follow!(&r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(&format!(
                    "SELECT u.id AS user_id, u.username, u.display_name, u.avatar_path,
                            DATE_FORMAT(f.created_at, '%Y-%m-%dT%H:%i:%sZ') AS followed_at
                     FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ? AND u.banned_at IS NULL
                       AND (u.username LIKE ? ESCAPE '\\\\' OR COALESCE(u.display_name, '') LIKE ? ESCAPE '\\\\')
                     ORDER BY f.created_at DESC
                     LIMIT ? OFFSET ?",
                ))
                .bind(user_id)
                .bind(pat)
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(&format!(
                    "SELECT u.id AS user_id, u.username, u.display_name, u.avatar_path,
                            DATE_FORMAT(f.created_at, '%Y-%m-%dT%H:%i:%sZ') AS followed_at
                     FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ? AND u.banned_at IS NULL
                     ORDER BY f.created_at DESC
                     LIMIT ? OFFSET ?",
                ))
                .bind(user_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list follows: {e}"))?;
            rows.into_iter().map(|r| Ok(map_follow!(&r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(&format!(
                    "SELECT u.id AS user_id, u.username, u.display_name, u.avatar_path,
                            strftime('%Y-%m-%dT%H:%M:%SZ', f.created_at) AS followed_at
                     FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ?1 AND u.banned_at IS NULL
                       AND (u.username LIKE ?2 ESCAPE '\\' OR COALESCE(u.display_name, '') LIKE ?2 ESCAPE '\\')
                     ORDER BY f.created_at DESC
                     LIMIT ?3 OFFSET ?4",
                ))
                .bind(user_id)
                .bind(pat)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query(&format!(
                    "SELECT u.id AS user_id, u.username, u.display_name, u.avatar_path,
                            strftime('%Y-%m-%dT%H:%M:%SZ', f.created_at) AS followed_at
                     FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ?1 AND u.banned_at IS NULL
                     ORDER BY f.created_at DESC
                     LIMIT ?2 OFFSET ?3",
                ))
                .bind(user_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list follows: {e}"))?;
            rows.into_iter().map(|r| Ok(map_follow!(&r))).collect()
        }
    }
}

/// Followers of `user_id`, newest first; `q` filters username/display_name.
pub async fn list_followers(
    pool: &DbPool,
    user_id: &str,
    q: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<UserFollowListRow>, String> {
    list_follow_edges(pool, user_id, true, q, offset, limit).await
}

/// Accounts `user_id` follows, newest first; `q` filters username/display_name.
pub async fn list_following(
    pool: &DbPool,
    user_id: &str,
    q: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<UserFollowListRow>, String> {
    list_follow_edges(pool, user_id, false, q, offset, limit).await
}

async fn count_follow_edges(
    pool: &DbPool,
    user_id: &str,
    followers: bool,
    q: Option<&str>,
) -> Result<i64, String> {
    let pat = q
        .filter(|s| !s.trim().is_empty())
        .map(|s| like_pat(s.trim()));
    let (edge_col, join_col) = if followers {
        ("f.followed_id", "f.follower_id")
    } else {
        ("f.follower_id", "f.followed_id")
    };
    match pool {
        DbPool::Postgres(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = $1 AND u.banned_at IS NULL
                       AND (u.username ILIKE $2 ESCAPE '\\' OR COALESCE(u.display_name, '') ILIKE $2 ESCAPE '\\')",
                ))
                .bind(user_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = $1 AND u.banned_at IS NULL"
                ))
                .bind(user_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count follows: {e}"))
        }
        DbPool::MySql(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ? AND u.banned_at IS NULL
                       AND (u.username LIKE ? ESCAPE '\\\\' OR COALESCE(u.display_name, '') LIKE ? ESCAPE '\\\\')",
                ))
                .bind(user_id)
                .bind(pat)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ? AND u.banned_at IS NULL"
                ))
                .bind(user_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count follows: {e}"))
        }
        DbPool::Sqlite(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ?1 AND u.banned_at IS NULL
                       AND (u.username LIKE ?2 ESCAPE '\\' OR COALESCE(u.display_name, '') LIKE ?2 ESCAPE '\\')",
                ))
                .bind(user_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM user_follows f
                     JOIN users u ON u.id = {join_col}
                     WHERE {edge_col} = ?1 AND u.banned_at IS NULL"
                ))
                .bind(user_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count follows: {e}"))
        }
    }
}

/// Follower count matching the `q` filter used by `list_followers`.
pub async fn count_followers(
    pool: &DbPool,
    user_id: &str,
    q: Option<&str>,
) -> Result<i64, String> {
    count_follow_edges(pool, user_id, true, q).await
}

/// Following count matching the `q` filter used by `list_following`.
pub async fn count_following(
    pool: &DbPool,
    user_id: &str,
    q: Option<&str>,
) -> Result<i64, String> {
    count_follow_edges(pool, user_id, false, q).await
}
