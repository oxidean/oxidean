//! Per-dialect sqlx migrator run + empty-target check (D-05, D-06, D-16).

use crate::pool::DbPool;

pub async fn run_migrations(pool: &DbPool) -> Result<(), String> {
    let res = match pool {
        DbPool::Postgres(p) => sqlx::migrate!("migrations/postgres").run(p).await,
        DbPool::MySql(p) => sqlx::migrate!("migrations/mysql").run(p).await,
        DbPool::Sqlite(p) => sqlx::migrate!("migrations/sqlite").run(p).await,
    };
    res.map_err(|e| format!("migration failed: {e}"))
}

/// Count user tables excluding sqlx's own bookkeeping table (D-16).
pub async fn is_empty(pool: &DbPool) -> Result<bool, String> {
    let count: i64 = match pool {
        DbPool::Postgres(p) => sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name <> '_sqlx_migrations'",
        )
        .fetch_one(p)
        .await
        .map_err(|e| format!("is_empty check failed: {e}"))?,
        DbPool::MySql(p) => sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM information_schema.tables WHERE table_schema = database() AND table_name <> '_sqlx_migrations'",
        )
        .fetch_one(p)
        .await
        .map_err(|e| format!("is_empty check failed: {e}"))?,
        DbPool::Sqlite(p) => sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'",
        )
        .fetch_one(p)
        .await
        .map_err(|e| format!("is_empty check failed: {e}"))?,
    };
    Ok(count == 0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    fn migration_files(dir: &str) -> BTreeSet<String> {
        let path = format!("{}/migrations/{}", env!("CARGO_MANIFEST_DIR"), dir);
        std::fs::read_dir(&path)
            .unwrap_or_else(|e| panic!("failed to read {path}: {e}"))
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn migration_parity() {
        let postgres = migration_files("postgres");
        let mysql = migration_files("mysql");
        let sqlite = migration_files("sqlite");

        assert!(
            !postgres.is_empty(),
            "postgres migrations must not be empty"
        );
        assert_eq!(
            postgres, mysql,
            "postgres and mysql migration sets diverged"
        );
        assert_eq!(
            postgres, sqlite,
            "postgres and sqlite migration sets diverged"
        );
    }
}
