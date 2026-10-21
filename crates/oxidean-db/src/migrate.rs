//! Per-dialect sqlx migrator run + empty-target check (D-05, D-06, D-16).

use crate::pool::DbPool;

pub async fn run_migrations(pool: &DbPool) -> Result<(), String> {
    reseat_renumbered_migrations(pool).await;
    let res = match pool {
        DbPool::Postgres(p) => sqlx::migrate!("migrations/postgres").run(p).await,
        DbPool::MySql(p) => sqlx::migrate!("migrations/mysql").run(p).await,
        DbPool::Sqlite(p) => sqlx::migrate!("migrations/sqlite").run(p).await,
    };
    res.map_err(|e| format!("migration failed: {e}"))
}

/// One-time repair for databases that applied this branch's migrations at
/// their pre-rebase versions (53-78): main's VCS schema landed at 53-55, so
/// the block renumbered to 56-81 with contents (and checksums) unchanged.
/// Re-seat the old rows once — detected by version 53 still holding
/// `action_job_payloads` — so sqlx's checksum validation passes and the VCS
/// trio can apply. No-op on fresh databases, on databases that never ran
/// this branch (prod has nothing ≥53), and after the repair succeeds (53
/// then holds `vcs_schema`). Safe to remove once no deployment can still be
/// carrying the pre-rebase numbering.
async fn reseat_renumbered_migrations(pool: &DbPool) {
    const RESEAT: &[&str] = &[
        // Hoist old rows out of the target range first so the second
        // statement's version writes can't collide with a live row.
        // sqlx stores descriptions with underscores as spaces.
        "UPDATE _sqlx_migrations SET version = version + 10000 \
         WHERE version >= 53 \
           AND EXISTS (SELECT 1 FROM _sqlx_migrations \
                       WHERE version = 53 AND description = 'action job payloads')",
        // Only hoisted rows can live ≥10053 — bring them down +3 net.
        "UPDATE _sqlx_migrations SET version = version - 9997 \
         WHERE version >= 10053",
    ];
    async fn run(pool: &DbPool) -> Result<(), sqlx::Error> {
        for sql in RESEAT {
            match pool {
                DbPool::Postgres(p) => sqlx::query(*sql).execute(p).await.map(|_| ())?,
                DbPool::MySql(p) => sqlx::query(*sql).execute(p).await.map(|_| ())?,
                DbPool::Sqlite(p) => sqlx::query(*sql).execute(p).await.map(|_| ())?,
            };
        }
        Ok(())
    }
    // Table not yet created (first boot) — nothing to re-seat.
    let _ = run(pool).await;
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
