//! Server-level administrative helpers for Postgres/MySQL (test harnesses and
//! tooling that need isolated databases on a shared server).

use crate::dialect::{self, Dialect};
use crate::pool::DbPool;

/// Create a fresh database on the server an admin URL points at and return a
/// connection URL for it (same credentials, new path).
///
/// `name` must be `[A-Za-z0-9_]{1,63}` — identifiers cannot be bind
/// parameters, so it is interpolated after validation. `admin_url` must use a
/// `postgres://`/`postgresql://` or `mysql://` scheme and carry credentials
/// with `CREATE DATABASE` privilege; its path is ignored for the returned URL
/// (any admin database works, e.g. `postgres://u:p@host/postgres`,
/// `mysql://root:p@host/`).
///
/// The created database is NOT dropped — this exists for harnesses running
/// against throwaway servers (CI services, ephemeral containers); callers that
/// need cleanup issue `DROP DATABASE` themselves.
pub async fn create_database(admin_url: &str, name: &str) -> Result<String, String> {
    let dialect = dialect::from_url(admin_url)?;
    if name.is_empty()
        || name.len() > 63
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(format!("invalid database name: {name:?}"));
    }
    let ddl = match dialect {
        Dialect::Postgres => format!("CREATE DATABASE \"{name}\""),
        Dialect::MySql => format!("CREATE DATABASE `{name}`"),
        Dialect::Sqlite => {
            return Err(
                "create_database applies to server engines; SQLite databases are files".into(),
            );
        }
    };
    // `ddl` is safe to interpolate: `name` is restricted to [A-Za-z0-9_] above.
    match DbPool::connect(admin_url, dialect).await? {
        DbPool::Postgres(p) => {
            sqlx::query(sqlx::AssertSqlSafe(&*ddl))
                .execute(&p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::MySql(p) => {
            sqlx::query(sqlx::AssertSqlSafe(&*ddl))
                .execute(&p)
                .await
                .map_err(|e| e.to_string())?;
        }
        DbPool::Sqlite(_) => unreachable!("sqlite rejected above"),
    }
    with_database_path(admin_url, name)
}

/// Rewrite `url` to point at database `name`: `scheme://authority/<name>[?query]`.
fn with_database_path(url: &str, name: &str) -> Result<String, String> {
    let scheme_end = url
        .find("://")
        .ok_or_else(|| format!("admin URL missing scheme: {url:?}"))?;
    let (head, rest) = url.split_at(scheme_end + 3);
    let (authority_path, query) = match rest.split_once('?') {
        Some((ap, q)) => (ap, format!("?{q}")),
        None => (rest, String::new()),
    };
    let authority = authority_path.split('/').next().unwrap_or_default();
    if authority.is_empty() {
        return Err(format!("admin URL missing host: {url:?}"));
    }
    Ok(format!("{head}{authority}/{name}{query}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_database_path() {
        assert_eq!(
            with_database_path("postgres://u:p@host:5432/postgres", "oxt_1").unwrap(),
            "postgres://u:p@host:5432/oxt_1"
        );
        assert_eq!(
            with_database_path("mysql://root:p@127.0.0.1:3306/", "oxt_2").unwrap(),
            "mysql://root:p@127.0.0.1:3306/oxt_2"
        );
        assert_eq!(
            with_database_path("mysql://root:p@h", "oxt_3").unwrap(),
            "mysql://root:p@h/oxt_3"
        );
        assert_eq!(
            with_database_path("postgres://u:p@h/db?sslmode=disable", "oxt_4").unwrap(),
            "postgres://u:p@h/oxt_4?sslmode=disable"
        );
    }
}
