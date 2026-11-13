//! `user_experiments` (migration 0100) — per-user experiment flags (WI-0001).
//! One row per enabled `(user_id, flag_key)`; absence means off. Keys are
//! validated against the `oxidean-core` registry by the RPC layer — the store
//! trusts its caller and just persists/deletes rows.

use crate::pool::DbPool;

/// Enabled flag keys for `user_id` (registry order is applied by callers).
pub async fn list_user_experiments(pool: &DbPool, user_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query_scalar("SELECT flag_key FROM user_experiments WHERE user_id = $1")
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list user experiments failed: {e}"))
        }
        DbPool::MySql(p) => {
            sqlx::query_scalar("SELECT flag_key FROM user_experiments WHERE user_id = ?")
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list user experiments failed: {e}"))
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar("SELECT flag_key FROM user_experiments WHERE user_id = ?1")
                .bind(user_id)
                .fetch_all(p)
                .await
                .map_err(|e| format!("list user experiments failed: {e}"))
        }
    }
}

/// Enable (`enabled = true`, idempotent insert) or disable (delete) a flag.
pub async fn set_user_experiment(
    pool: &DbPool,
    user_id: &str,
    flag_key: &str,
    enabled: bool,
) -> Result<(), String> {
    if enabled {
        match pool {
            DbPool::Postgres(p) => sqlx::query(
                "INSERT INTO user_experiments (user_id, flag_key) VALUES ($1, $2) \
                 ON CONFLICT (user_id, flag_key) DO NOTHING",
            )
            .bind(user_id)
            .bind(flag_key)
            .execute(p)
            .await
            .map_err(|e| format!("enable user experiment failed: {e}"))
            .map(|_| ()),
            DbPool::MySql(p) => {
                sqlx::query("INSERT IGNORE INTO user_experiments (user_id, flag_key) VALUES (?, ?)")
                    .bind(user_id)
                    .bind(flag_key)
                    .execute(p)
                    .await
                    .map_err(|e| format!("enable user experiment failed: {e}"))
                    .map(|_| ())
            }
            DbPool::Sqlite(p) => sqlx::query(
                "INSERT OR IGNORE INTO user_experiments (user_id, flag_key) VALUES (?1, ?2)",
            )
            .bind(user_id)
            .bind(flag_key)
            .execute(p)
            .await
            .map_err(|e| format!("enable user experiment failed: {e}"))
            .map(|_| ()),
        }
    } else {
        match pool {
            DbPool::Postgres(p) => {
                sqlx::query("DELETE FROM user_experiments WHERE user_id = $1 AND flag_key = $2")
                    .bind(user_id)
                    .bind(flag_key)
                    .execute(p)
                    .await
                    .map_err(|e| format!("disable user experiment failed: {e}"))
                    .map(|_| ())
            }
            DbPool::MySql(p) => {
                sqlx::query("DELETE FROM user_experiments WHERE user_id = ? AND flag_key = ?")
                    .bind(user_id)
                    .bind(flag_key)
                    .execute(p)
                    .await
                    .map_err(|e| format!("disable user experiment failed: {e}"))
                    .map(|_| ())
            }
            DbPool::Sqlite(p) => {
                sqlx::query("DELETE FROM user_experiments WHERE user_id = ?1 AND flag_key = ?2")
                    .bind(user_id)
                    .bind(flag_key)
                    .execute(p)
                    .await
                    .map_err(|e| format!("disable user experiment failed: {e}"))
                    .map(|_| ())
            }
        }
    }
}
