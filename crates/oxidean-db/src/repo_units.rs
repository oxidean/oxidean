//! Per-repo unit enable flags (COL-13): `issues_enabled` / `pulls_enabled`.
//!
//! Disabling a unit never deletes data — the `issue.*` / `pull.*` RPC acl
//! resolvers reject calls while the flag is off and rows resume on re-enable.
//! Extend to future units (wiki, boards, releases, packages) by adding a
//! `<unit>_enabled` column on `repositories` plus a flag + setter pair here.

use sqlx::Row;

use crate::pool::DbPool;

/// Snapshot of the per-repo unit flags (COL-13). Missing rows / columns fall
/// back to enabled — `repositories` rows pre-dating migration 0043 default on.
#[derive(Clone, Copy, Debug)]
pub struct RepoUnitFlags {
    pub issues_enabled: bool,
    pub pulls_enabled: bool,
}

impl Default for RepoUnitFlags {
    fn default() -> Self {
        Self {
            issues_enabled: true,
            pulls_enabled: true,
        }
    }
}

pub async fn get_unit_flags(pool: &DbPool, repo_id: &str) -> Result<RepoUnitFlags, String> {
    match pool {
        DbPool::Sqlite(p) => {
            let row =
                sqlx::query("SELECT issues_enabled, pulls_enabled FROM repositories WHERE id = ?")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| RepoUnitFlags {
                    issues_enabled: r.try_get::<i64, _>("issues_enabled").unwrap_or(1) != 0,
                    pulls_enabled: r.try_get::<i64, _>("pulls_enabled").unwrap_or(1) != 0,
                })
                .unwrap_or_default())
        }
        DbPool::Postgres(p) => {
            let row =
                sqlx::query("SELECT issues_enabled, pulls_enabled FROM repositories WHERE id = $1")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| RepoUnitFlags {
                    issues_enabled: r.try_get::<bool, _>("issues_enabled").unwrap_or(true),
                    pulls_enabled: r.try_get::<bool, _>("pulls_enabled").unwrap_or(true),
                })
                .unwrap_or_default())
        }
        DbPool::MySql(p) => {
            let row =
                sqlx::query("SELECT issues_enabled, pulls_enabled FROM repositories WHERE id = ?")
                    .bind(repo_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(row
                .map(|r| RepoUnitFlags {
                    issues_enabled: r.try_get::<i8, _>("issues_enabled").unwrap_or(1) != 0,
                    pulls_enabled: r.try_get::<i8, _>("pulls_enabled").unwrap_or(1) != 0,
                })
                .unwrap_or_default())
        }
    }
}

pub async fn set_issues_enabled(pool: &DbPool, repo_id: &str, enabled: bool) -> Result<(), String> {
    set_unit_flag(pool, repo_id, "issues_enabled", enabled).await
}

pub async fn set_pulls_enabled(pool: &DbPool, repo_id: &str, enabled: bool) -> Result<(), String> {
    set_unit_flag(pool, repo_id, "pulls_enabled", enabled).await
}

async fn set_unit_flag(
    pool: &DbPool,
    repo_id: &str,
    column: &'static str,
    enabled: bool,
) -> Result<(), String> {
    // `column` is an internal literal, never caller input — safe to inline.
    let v = if enabled { 1i64 } else { 0 };
    match pool {
        DbPool::Sqlite(p) => {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE repositories SET {column} = ? WHERE id = ?"
            )))
            .bind(v)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::Postgres(p) => {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE repositories SET {column} = $1 WHERE id = $2"
            )))
            .bind(enabled)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE repositories SET {column} = ? WHERE id = ?"
            )))
            .bind(v as i8)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
