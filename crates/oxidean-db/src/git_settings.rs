//! Singleton instance git settings (id = 1) — git object size quota default (GIT-25).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone, Default)]
pub struct GitSettingsRow {
    /// Instance default per-repo bare-size quota in bytes.
    /// NULL = use `OXIDEAN_GIT_REPO_QUOTA_BYTES` env / built-in default.
    /// <= 0 is stored but treated as unlimited by the enforcement path.
    pub repo_quota_bytes: Option<i64>,
}

pub async fn get_git_settings(pool: &DbPool) -> Result<GitSettingsRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row =
                sqlx::query("SELECT repo_quota_bytes FROM instance_git_settings WHERE id = 1")
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("get_git_settings: {e}"))?;
            Ok(GitSettingsRow {
                repo_quota_bytes: row
                    .try_get::<Option<i64>, _>("repo_quota_bytes")
                    .unwrap_or(None),
            })
        }
        DbPool::Postgres(p) => {
            let row =
                sqlx::query("SELECT repo_quota_bytes FROM instance_git_settings WHERE id = 1")
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("get_git_settings: {e}"))?;
            Ok(GitSettingsRow {
                repo_quota_bytes: row
                    .try_get::<Option<i64>, _>("repo_quota_bytes")
                    .unwrap_or(None),
            })
        }
        DbPool::MySql(p) => {
            let row =
                sqlx::query("SELECT repo_quota_bytes FROM instance_git_settings WHERE id = 1")
                    .fetch_one(p)
                    .await
                    .map_err(|e| format!("get_git_settings: {e}"))?;
            Ok(GitSettingsRow {
                repo_quota_bytes: row
                    .try_get::<Option<i64>, _>("repo_quota_bytes")
                    .unwrap_or(None),
            })
        }
    }
}

/// Persist the instance default override (`None` reverts to env/built-in default).
pub async fn update_git_settings(
    pool: &DbPool,
    repo_quota_bytes: Option<i64>,
) -> Result<GitSettingsRow, String> {
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE instance_git_settings SET
                   repo_quota_bytes = ?,
                   updated_at = strftime('%Y-%m-%d %H:%M:%S','now')
                 WHERE id = 1",
            )
            .bind(repo_quota_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update_git_settings: {e}"))?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE instance_git_settings SET
                   repo_quota_bytes = $1,
                   updated_at = NOW()
                 WHERE id = 1",
            )
            .bind(repo_quota_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update_git_settings: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE instance_git_settings SET
                   repo_quota_bytes = ?,
                   updated_at = CURRENT_TIMESTAMP(3)
                 WHERE id = 1",
            )
            .bind(repo_quota_bytes)
            .execute(p)
            .await
            .map_err(|e| format!("update_git_settings: {e}"))?;
        }
    }
    get_git_settings(pool).await
}
