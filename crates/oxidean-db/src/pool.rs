//! Concrete sqlx pools behind one enum (D-08). SQLite: parent dirs, WAL, FKs (D-18, D-20).

use std::str::FromStr;

use sqlx::mysql::MySqlPoolOptions;
use sqlx::postgres::PgPoolOptions;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use crate::dialect::{redact_url, Dialect};

#[derive(Clone)]
pub enum DbPool {
    Postgres(sqlx::PgPool),
    MySql(sqlx::MySqlPool),
    Sqlite(sqlx::SqlitePool),
}

impl DbPool {
    pub async fn connect(url: &str, dialect: Dialect) -> Result<DbPool, String> {
        match dialect {
            Dialect::Postgres => PgPoolOptions::new()
                .max_connections(5)
                .connect(url)
                .await
                .map(DbPool::Postgres)
                .map_err(|e| format!("{}: {e}", redact_url(url))),
            Dialect::MySql => MySqlPoolOptions::new()
                .max_connections(5)
                .connect(url)
                .await
                .map(DbPool::MySql)
                .map_err(|e| format!("{}: {e}", redact_url(url))),
            Dialect::Sqlite => {
                ensure_sqlite_parent_dir(url)?;
                let opts = SqliteConnectOptions::from_str(url)
                    .map_err(|e| format!("{}: {e}", redact_url(url)))?
                    .create_if_missing(true)
                    .journal_mode(SqliteJournalMode::Wal)
                    .foreign_keys(true)
                    .busy_timeout(std::time::Duration::from_secs(5));
                SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_with(opts)
                    .await
                    .map(DbPool::Sqlite)
                    .map_err(|e| format!("{}: {e}", redact_url(url)))
            }
        }
    }

    pub fn dialect_of(pool: &DbPool) -> Dialect {
        match pool {
            DbPool::Postgres(_) => Dialect::Postgres,
            DbPool::MySql(_) => Dialect::MySql,
            DbPool::Sqlite(_) => Dialect::Sqlite,
        }
    }
}

/// Create parent directories for on-disk SQLite URLs (sqlx only creates the file).
pub fn ensure_sqlite_parent_dir(url: &str) -> Result<(), String> {
    let Some(path_part) = url.strip_prefix("sqlite:") else {
        return Ok(());
    };
    let path_part = path_part.strip_prefix("//").unwrap_or(path_part);
    let path_part = path_part.split('?').next().unwrap_or(path_part);
    if path_part.is_empty() || path_part == ":memory:" {
        return Ok(());
    }
    let path = std::path::Path::new(path_part);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "failed to create SQLite parent dir {}: {e}",
                    parent.display()
                )
            })?;
        }
    }
    Ok(())
}
