//! `user_pinned_repos` (migration 0096) and `org_pinned_repos` (0098) —
//! pinned repositories on user/org profiles (COL-04). One row per pin keyed
//! `(owner, repository_id)` with a per-owner unique `position` carrying
//! display order (0-based, contiguous — `set` rewrites the list atomically).

use crate::pool::DbPool;

/// Pinned repository ids in display order (`position` ascending).
pub async fn list_pinned_repo_ids(pool: &DbPool, user_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT repository_id FROM user_pinned_repos WHERE user_id = $1 ORDER BY position",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list pinned repos failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT repository_id FROM user_pinned_repos WHERE user_id = ? ORDER BY position",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list pinned repos failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT repository_id FROM user_pinned_repos WHERE user_id = ?1 ORDER BY position",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list pinned repos failed: {e}")),
    }
}

/// Replace a user's pin list atomically — delete + ordered re-insert in one
/// transaction so a mid-write read never sees a partial list.
pub async fn set_pinned_repos(
    pool: &DbPool,
    user_id: &str,
    repository_ids: &[String],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM user_pinned_repos WHERE user_id = $1")
                .bind(user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear pinned repos failed: {e}"))?;
            for (i, id) in repository_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO user_pinned_repos (user_id, repository_id, position) VALUES ($1, $2, $3)",
                )
                .bind(user_id)
                .bind(id)
                .bind(i as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert pinned repo failed: {e}"))?;
            }
            tx.commit().await.map_err(|e| e.to_string())
        }
        DbPool::MySql(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM user_pinned_repos WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear pinned repos failed: {e}"))?;
            for (i, id) in repository_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO user_pinned_repos (user_id, repository_id, position) VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(id)
                .bind(i as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert pinned repo failed: {e}"))?;
            }
            tx.commit().await.map_err(|e| e.to_string())
        }
        DbPool::Sqlite(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM user_pinned_repos WHERE user_id = ?1")
                .bind(user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear pinned repos failed: {e}"))?;
            for (i, id) in repository_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO user_pinned_repos (user_id, repository_id, position) VALUES (?1, ?2, ?3)",
                )
                .bind(user_id)
                .bind(id)
                .bind(i as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert pinned repo failed: {e}"))?;
            }
            tx.commit().await.map_err(|e| e.to_string())
        }
    }
}

/// Pinned repository ids for an org profile, display order (`position`
/// ascending).
pub async fn list_org_pinned_repo_ids(pool: &DbPool, org_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => sqlx::query_scalar(
            "SELECT repository_id FROM org_pinned_repos WHERE org_id = $1 ORDER BY position",
        )
        .bind(org_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list org pinned repos failed: {e}")),
        DbPool::MySql(p) => sqlx::query_scalar(
            "SELECT repository_id FROM org_pinned_repos WHERE org_id = ? ORDER BY position",
        )
        .bind(org_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list org pinned repos failed: {e}")),
        DbPool::Sqlite(p) => sqlx::query_scalar(
            "SELECT repository_id FROM org_pinned_repos WHERE org_id = ?1 ORDER BY position",
        )
        .bind(org_id)
        .fetch_all(p)
        .await
        .map_err(|e| format!("list org pinned repos failed: {e}")),
    }
}

/// Replace an org's pin list atomically — delete + ordered re-insert in one
/// transaction so a mid-write read never sees a partial list.
pub async fn set_org_pinned_repos(
    pool: &DbPool,
    org_id: &str,
    repository_ids: &[String],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM org_pinned_repos WHERE org_id = $1")
                .bind(org_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear org pinned repos failed: {e}"))?;
            for (i, id) in repository_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO org_pinned_repos (org_id, repository_id, position) VALUES ($1, $2, $3)",
                )
                .bind(org_id)
                .bind(id)
                .bind(i as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert org pinned repo failed: {e}"))?;
            }
            tx.commit().await.map_err(|e| e.to_string())
        }
        DbPool::MySql(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM org_pinned_repos WHERE org_id = ?")
                .bind(org_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear org pinned repos failed: {e}"))?;
            for (i, id) in repository_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO org_pinned_repos (org_id, repository_id, position) VALUES (?, ?, ?)",
                )
                .bind(org_id)
                .bind(id)
                .bind(i as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert org pinned repo failed: {e}"))?;
            }
            tx.commit().await.map_err(|e| e.to_string())
        }
        DbPool::Sqlite(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM org_pinned_repos WHERE org_id = ?1")
                .bind(org_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("clear org pinned repos failed: {e}"))?;
            for (i, id) in repository_ids.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO org_pinned_repos (org_id, repository_id, position) VALUES (?1, ?2, ?3)",
                )
                .bind(org_id)
                .bind(id)
                .bind(i as i64)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert org pinned repo failed: {e}"))?;
            }
            tx.commit().await.map_err(|e| e.to_string())
        }
    }
}
