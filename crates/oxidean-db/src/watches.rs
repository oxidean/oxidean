//! Repository watches — mirrors stars.rs (issue #23).

use crate::pool::DbPool;

/// Upsert the subscription row at `level` (`all` | `participating` | `ignore`).
/// `watch_count` counts only active (non-ignore) subscriptions, so the counter
/// bump is the delta between the prior and new active state (issue #23 / DEBT-06).
pub async fn watch_repository(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
    level: &str,
) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p.begin().await.map_err(|e| format!("watch begin: {e}"))?;
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT level FROM repository_watches
                 WHERE user_id = $1 AND repository_id = $2",
            )
            .bind(user_id)
            .bind(repository_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("watch lookup: {e}"))?;
            sqlx::query(
                "INSERT INTO repository_watches (user_id, repository_id, level)
                 VALUES ($1, $2, $3)
                 ON CONFLICT (user_id, repository_id) DO UPDATE SET level = EXCLUDED.level",
            )
            .bind(user_id)
            .bind(repository_id)
            .bind(level)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("watch upsert: {e}"))?;
            let delta = watch_delta(prev.as_deref(), level);
            if delta != 0 {
                sqlx::query(
                    "UPDATE repositories SET watch_count = GREATEST(watch_count + $1, 0), updated_at = now()
                     WHERE id = $2 AND deleted_at IS NULL",
                )
                .bind(delta)
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("watch bump: {e}"))?;
            }
            let count: i64 =
                sqlx::query_scalar("SELECT watch_count FROM repositories WHERE id = $1")
                    .bind(repository_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| format!("watch count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("watch commit: {e}"))?;
            Ok(count)
        }
        DbPool::MySql(p) => {
            let mut tx = p.begin().await.map_err(|e| format!("watch begin: {e}"))?;
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT level FROM repository_watches WHERE user_id = ? AND repository_id = ?",
            )
            .bind(user_id)
            .bind(repository_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("watch lookup: {e}"))?;
            sqlx::query(
                "INSERT INTO repository_watches (user_id, repository_id, level)
                 VALUES (?, ?, ?)
                 ON DUPLICATE KEY UPDATE level = VALUES(level)",
            )
            .bind(user_id)
            .bind(repository_id)
            .bind(level)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("watch upsert: {e}"))?;
            let delta = watch_delta(prev.as_deref(), level);
            if delta != 0 {
                sqlx::query(
                    "UPDATE repositories SET watch_count = GREATEST(watch_count + ?, 0), updated_at = NOW()
                     WHERE id = ? AND deleted_at IS NULL",
                )
                .bind(delta)
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("watch bump: {e}"))?;
            }
            let count: i64 =
                sqlx::query_scalar("SELECT watch_count FROM repositories WHERE id = ?")
                    .bind(repository_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| format!("watch count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("watch commit: {e}"))?;
            Ok(count)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p.begin().await.map_err(|e| format!("watch begin: {e}"))?;
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT level FROM repository_watches
                 WHERE user_id = ?1 AND repository_id = ?2",
            )
            .bind(user_id)
            .bind(repository_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("watch lookup: {e}"))?;
            sqlx::query(
                "INSERT INTO repository_watches (user_id, repository_id, level)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT (user_id, repository_id) DO UPDATE SET level = excluded.level",
            )
            .bind(user_id)
            .bind(repository_id)
            .bind(level)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("watch upsert: {e}"))?;
            let delta = watch_delta(prev.as_deref(), level);
            if delta != 0 {
                sqlx::query(
                    "UPDATE repositories SET watch_count = MAX(watch_count + ?1, 0),
                     updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                     WHERE id = ?2 AND deleted_at IS NULL",
                )
                .bind(delta)
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("watch bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT watch_count FROM repositories WHERE id = ?1",
            )
            .bind(repository_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("watch count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("watch commit: {e}"))?;
            Ok(count)
        }
    }
}

/// Active-state delta between a prior and new watch level. Only non-`ignore`
/// levels count toward `watch_count` / `viewer_is_watching`.
fn watch_delta(prev: Option<&str>, next: &str) -> i64 {
    i64::from(next != "ignore") - i64::from(prev.is_some_and(|l| l != "ignore"))
}

/// Idempotent unwatch: delete membership and decrement counter when a row was removed.
pub async fn unwatch_repository(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p.begin().await.map_err(|e| format!("unwatch begin: {e}"))?;
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT level FROM repository_watches
                 WHERE user_id = $1 AND repository_id = $2",
            )
            .bind(user_id)
            .bind(repository_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("unwatch lookup: {e}"))?;
            if prev.is_some() {
                sqlx::query(
                    "DELETE FROM repository_watches WHERE user_id = $1 AND repository_id = $2",
                )
                .bind(user_id)
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unwatch delete: {e}"))?;
            }
            // Only previously-active (non-ignore) rows count toward watch_count.
            if prev.as_deref().is_some_and(|l| l != "ignore") {
                sqlx::query(
                    "UPDATE repositories SET watch_count = GREATEST(watch_count - 1, 0),
                     updated_at = now() WHERE id = $1 AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unwatch bump: {e}"))?;
            }
            let count: i64 =
                sqlx::query_scalar("SELECT watch_count FROM repositories WHERE id = $1")
                    .bind(repository_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| format!("unwatch count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("unwatch commit: {e}"))?;
            Ok(count)
        }
        DbPool::MySql(p) => {
            let mut tx = p.begin().await.map_err(|e| format!("unwatch begin: {e}"))?;
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT level FROM repository_watches WHERE user_id = ? AND repository_id = ?",
            )
            .bind(user_id)
            .bind(repository_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("unwatch lookup: {e}"))?;
            if prev.is_some() {
                sqlx::query(
                    "DELETE FROM repository_watches WHERE user_id = ? AND repository_id = ?",
                )
                .bind(user_id)
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unwatch delete: {e}"))?;
            }
            if prev.as_deref().is_some_and(|l| l != "ignore") {
                sqlx::query(
                    "UPDATE repositories SET watch_count = GREATEST(watch_count - 1, 0),
                     updated_at = NOW() WHERE id = ? AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unwatch bump: {e}"))?;
            }
            let count: i64 =
                sqlx::query_scalar("SELECT watch_count FROM repositories WHERE id = ?")
                    .bind(repository_id)
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| format!("unwatch count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("unwatch commit: {e}"))?;
            Ok(count)
        }
        DbPool::Sqlite(p) => {
            let mut tx = p.begin().await.map_err(|e| format!("unwatch begin: {e}"))?;
            let prev: Option<String> = sqlx::query_scalar(
                "SELECT level FROM repository_watches
                 WHERE user_id = ?1 AND repository_id = ?2",
            )
            .bind(user_id)
            .bind(repository_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("unwatch lookup: {e}"))?;
            if prev.is_some() {
                sqlx::query(
                    "DELETE FROM repository_watches WHERE user_id = ?1 AND repository_id = ?2",
                )
                .bind(user_id)
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unwatch delete: {e}"))?;
            }
            if prev.as_deref().is_some_and(|l| l != "ignore") {
                sqlx::query(
                    "UPDATE repositories SET watch_count = MAX(watch_count - 1, 0),
                     updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                     WHERE id = ?1 AND deleted_at IS NULL",
                )
                .bind(repository_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("unwatch bump: {e}"))?;
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT watch_count FROM repositories WHERE id = ?1",
            )
            .bind(repository_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| format!("unwatch count: {e}"))?;
            tx.commit()
                .await
                .map_err(|e| format!("unwatch commit: {e}"))?;
            Ok(count)
        }
    }
}

pub async fn get_watch_count(pool: &DbPool, repository_id: &str) -> Result<i64, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT COALESCE(watch_count, 0) FROM repositories WHERE id = $1")
                .bind(repository_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get watch_count: {e}"))
                .map(|o| o.unwrap_or(0))
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT COALESCE(watch_count, 0) FROM repositories WHERE id = ?")
                .bind(repository_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get watch_count: {e}"))
                .map(|o| o.unwrap_or(0))
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT COALESCE(watch_count, 0) FROM repositories WHERE id = ?1")
                .bind(repository_id)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("get watch_count: {e}"))
                .map(|o| o.unwrap_or(0))
        }
    }
}

pub async fn has_watched(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
) -> Result<bool, String> {
    let count: i64 = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repository_watches WHERE user_id = $1 AND repository_id = $2 AND level <> 'ignore'",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("has_watched: {e}"))?,
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repository_watches WHERE user_id = ? AND repository_id = ? AND level <> 'ignore'",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("has_watched: {e}"))?,
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM repository_watches WHERE user_id = ?1 AND repository_id = ?2 AND level <> 'ignore'",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_one(p)
        .await
        .map_err(|e| format!("has_watched: {e}"))?,
    };
    Ok(count > 0)
}

/// Watcher row for `repo.watchers.list` (no email).
#[derive(Debug, Clone)]
pub struct RepoWatcherListRow {
    pub user_id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_path: Option<String>,
    pub watched_at: String,
}

fn like_pat(q: &str) -> String {
    let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
    format!("%{escaped}%")
}

macro_rules! map_watcher {
    ($row:expr) => {{
        let row = $row;
        RepoWatcherListRow {
            user_id: row.try_get("user_id").map_err(|e| format!("watcher row: {e}"))?,
            username: row.try_get("username").map_err(|e| format!("watcher row: {e}"))?,
            display_name: row
                .try_get("display_name")
                .map_err(|e| format!("watcher row: {e}"))?,
            avatar_path: row
                .try_get("avatar_path")
                .map_err(|e| format!("watcher row: {e}"))?,
            watched_at: row
                .try_get("watched_at")
                .map_err(|e| format!("watcher row: {e}"))?,
        }
    }};
}

/// Paginated watchers for a repository, newest first. Optional `q` filters username/display_name.
pub async fn list_repo_watchers(
    pool: &DbPool,
    repository_id: &str,
    q: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<RepoWatcherListRow>, String> {
    use sqlx::Row;
    let pat = q.filter(|s| !s.trim().is_empty()).map(|s| like_pat(s.trim()));
    match pool {
        DbPool::Postgres(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(
                    "SELECT w.user_id, u.username, u.display_name, u.avatar_path,
                            to_char(w.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS watched_at
                     FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = $1 AND w.level <> 'ignore'
                       AND (u.username ILIKE $2 ESCAPE '\\' OR COALESCE(u.display_name, '') ILIKE $2 ESCAPE '\\')
                     ORDER BY w.created_at DESC
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
                    "SELECT w.user_id, u.username, u.display_name, u.avatar_path,
                            to_char(w.created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS watched_at
                     FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = $1 AND w.level <> 'ignore'
                     ORDER BY w.created_at DESC
                     LIMIT $2 OFFSET $3",
                )
                .bind(repository_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list watchers: {e}"))?;
            rows.into_iter().map(|r| Ok(map_watcher!(&r))).collect()
        }
        DbPool::MySql(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(
                    "SELECT w.user_id, u.username, u.display_name, u.avatar_path,
                            DATE_FORMAT(w.created_at, '%Y-%m-%dT%H:%i:%sZ') AS watched_at
                     FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = ? AND w.level <> 'ignore'
                       AND (u.username LIKE ? ESCAPE '\\\\' OR COALESCE(u.display_name, '') LIKE ? ESCAPE '\\\\')
                     ORDER BY w.created_at DESC
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
                    "SELECT w.user_id, u.username, u.display_name, u.avatar_path,
                            DATE_FORMAT(w.created_at, '%Y-%m-%dT%H:%i:%sZ') AS watched_at
                     FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = ? AND w.level <> 'ignore'
                     ORDER BY w.created_at DESC
                     LIMIT ? OFFSET ?",
                )
                .bind(repository_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list watchers: {e}"))?;
            rows.into_iter().map(|r| Ok(map_watcher!(&r))).collect()
        }
        DbPool::Sqlite(p) => {
            let rows = if let Some(ref pat) = pat {
                sqlx::query(
                    "SELECT w.user_id, u.username, u.display_name, u.avatar_path,
                            strftime('%Y-%m-%dT%H:%M:%SZ', w.created_at) AS watched_at
                     FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = ?1 AND w.level <> 'ignore'
                       AND (u.username LIKE ?2 ESCAPE '\\' OR COALESCE(u.display_name, '') LIKE ?2 ESCAPE '\\')
                     ORDER BY w.created_at DESC
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
                    "SELECT w.user_id, u.username, u.display_name, u.avatar_path,
                            strftime('%Y-%m-%dT%H:%M:%SZ', w.created_at) AS watched_at
                     FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = ?1 AND w.level <> 'ignore'
                     ORDER BY w.created_at DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .bind(repository_id)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
            .map_err(|e| format!("list watchers: {e}"))?;
            rows.into_iter().map(|r| Ok(map_watcher!(&r))).collect()
        }
    }
}

/// Count watchers matching optional `q`.
pub async fn count_repo_watchers(
    pool: &DbPool,
    repository_id: &str,
    q: Option<&str>,
) -> Result<i64, String> {
    let pat = q.filter(|s| !s.trim().is_empty()).map(|s| like_pat(s.trim()));
    match pool {
        DbPool::Postgres(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = $1 AND w.level <> 'ignore'
                       AND (u.username ILIKE $2 ESCAPE '\\' OR COALESCE(u.display_name, '') ILIKE $2 ESCAPE '\\')",
                )
                .bind(repository_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_watches WHERE repository_id = $1 AND level <> 'ignore'",
                )
                .bind(repository_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count watchers: {e}"))
        }
        DbPool::MySql(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = ? AND w.level <> 'ignore'
                       AND (u.username LIKE ? ESCAPE '\\\\' OR COALESCE(u.display_name, '') LIKE ? ESCAPE '\\\\')",
                )
                .bind(repository_id)
                .bind(pat)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_watches WHERE repository_id = ? AND level <> 'ignore'",
                )
                .bind(repository_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count watchers: {e}"))
        }
        DbPool::Sqlite(p) => {
            if let Some(ref pat) = pat {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_watches w
                     JOIN users u ON u.id = w.user_id
                     WHERE w.repository_id = ?1 AND w.level <> 'ignore'
                       AND (u.username LIKE ?2 ESCAPE '\\' OR COALESCE(u.display_name, '') LIKE ?2 ESCAPE '\\')",
                )
                .bind(repository_id)
                .bind(pat)
                .fetch_one(p)
                .await
            } else {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM repository_watches WHERE repository_id = ?1 AND level <> 'ignore'",
                )
                .bind(repository_id)
                .fetch_one(p)
                .await
            }
            .map_err(|e| format!("count watchers: {e}"))
        }
    }
}

/// Viewer's subscription level for a repo — `all` | `participating` | `ignore`;
/// `None` when no row exists (DEBT-06).
pub async fn get_watch_level(
    pool: &DbPool,
    user_id: &str,
    repository_id: &str,
) -> Result<Option<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT level FROM repository_watches WHERE user_id = $1 AND repository_id = $2",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_watch_level: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT level FROM repository_watches WHERE user_id = ? AND repository_id = ?",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_watch_level: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT level FROM repository_watches WHERE user_id = ?1 AND repository_id = ?2",
        )
        .bind(user_id)
        .bind(repository_id)
        .fetch_optional(p)
        .await
        .map_err(|e| format!("get_watch_level: {e}")),
    }
}

/// `(user_id, level)` for every subscription row on a repo — one indexed read
/// for the notification fan-out matrix (DEBT-06).
pub async fn list_repo_watch_levels(
    pool: &DbPool,
    repository_id: &str,
) -> Result<Vec<(String, String)>, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_as("SELECT user_id, level FROM repository_watches WHERE repository_id = $1")
                .bind(repository_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list watch levels: {e}"))
        }
        DbPool::MySql(p) => {
            sqlx::query_as("SELECT user_id, level FROM repository_watches WHERE repository_id = ?")
                .bind(repository_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list watch levels: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query_as("SELECT user_id, level FROM repository_watches WHERE repository_id = ?1")
                .bind(repository_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list watch levels: {e}"))
        }
    }
}

/// Repo ids with a subscription row at any level, newest first — powers the
/// settings notification matrix (`ignore` rows included so they stay
/// manageable) (DEBT-06).
pub async fn list_watched_repo_ids(
    pool: &DbPool,
    user_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT repository_id FROM repository_watches
             WHERE user_id = $1
             ORDER BY created_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list watched: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT repository_id FROM repository_watches
             WHERE user_id = ?
             ORDER BY created_at DESC
             LIMIT ? OFFSET ?",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list watched: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT repository_id FROM repository_watches
             WHERE user_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3",
        )
        .bind(user_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list watched: {e}")),
    }
}
