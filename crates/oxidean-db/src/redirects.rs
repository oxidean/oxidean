//! `repository_redirects` helpers (Phase 15 / D-REL-08).

use crate::pool::DbPool;
use sqlx::Row;

#[derive(Debug, Clone)]
pub struct RedirectRow {
    pub id: String,
    pub old_owner_slug: String,
    pub old_name: String,
    pub repo_id: String,
    pub expires_at: String,
    pub created_at: String,
}

macro_rules! map_redirect {
    ($row:expr) => {{
        let row = $row;
        RedirectRow {
            id: row.try_get("id").map_err(|e| format!("id: {e}"))?,
            old_owner_slug: row
                .try_get("old_owner_slug")
                .map_err(|e| format!("slug: {e}"))?,
            old_name: row.try_get("old_name").map_err(|e| format!("name: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("repo_id: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("expires: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("created: {e}"))?,
        }
    }};
}

const SEL_PG: &str = "SELECT id, old_owner_slug, old_name, repo_id, to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS expires_at, to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at FROM repository_redirects";
const SEL_MY: &str = "SELECT id, old_owner_slug, old_name, repo_id, DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at FROM repository_redirects";
const SEL_SQ: &str = "SELECT id, old_owner_slug, old_name, repo_id, strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) AS expires_at, strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at FROM repository_redirects";

pub async fn insert_redirect(
    pool: &DbPool,
    id: &str,
    old_owner_slug: &str,
    old_name: &str,
    repo_id: &str,
    expires_at: &str,
) -> Result<RedirectRow, String> {
    let slug = old_owner_slug.to_ascii_lowercase();
    let name = old_name.to_ascii_lowercase();
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("INSERT INTO repository_redirects (id, old_owner_slug, old_name, repo_id, expires_at) VALUES ($1,$2,$3,$4,$5::timestamptz)")
                .bind(id).bind(&slug).bind(&name).bind(repo_id).bind(expires_at)
                .execute(p).await.map_err(|e| format!("insert redirect: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("INSERT INTO repository_redirects (id, old_owner_slug, old_name, repo_id, expires_at) VALUES (?,?,?,?,STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))")
                .bind(id).bind(&slug).bind(&name).bind(repo_id).bind(expires_at)
                .execute(p).await.map_err(|e| format!("insert redirect: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let stored = expires_at.trim_end_matches('Z').replace('T', " ");
            sqlx::query("INSERT INTO repository_redirects (id, old_owner_slug, old_name, repo_id, expires_at) VALUES (?,?,?,?,?)")
                .bind(id).bind(&slug).bind(&name).bind(repo_id).bind(&stored)
                .execute(p).await.map_err(|e| format!("insert redirect: {e}"))?;
        }
    }
    find_redirect(pool, &slug, &name)
        .await?
        .ok_or_else(|| "redirect missing after insert".into())
}

pub async fn find_redirect(
    pool: &DbPool,
    old_owner_slug: &str,
    old_name: &str,
) -> Result<Option<RedirectRow>, String> {
    let slug = old_owner_slug.to_ascii_lowercase();
    let name = old_name.to_ascii_lowercase();
    match pool {
        DbPool::Postgres(p) => {
            let sql = format!("{SEL_PG} WHERE old_owner_slug = $1 AND old_name = $2");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(&slug)
                .bind(&name)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find redirect: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_redirect!(r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let sql = format!("{SEL_MY} WHERE old_owner_slug = ? AND old_name = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(&slug)
                .bind(&name)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find redirect: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_redirect!(r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let sql = format!("{SEL_SQ} WHERE old_owner_slug = ? AND old_name = ?");
            let row = sqlx::query(sqlx::AssertSqlSafe(&*sql))
                .bind(&slug)
                .bind(&name)
                .fetch_optional(p)
                .await
                .map_err(|e| format!("find redirect: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_redirect!(r))),
                None => Ok(None),
            }
        }
    }
}

pub async fn delete_redirect(
    pool: &DbPool,
    old_owner_slug: &str,
    old_name: &str,
) -> Result<(), String> {
    let slug = old_owner_slug.to_ascii_lowercase();
    let name = old_name.to_ascii_lowercase();
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM repository_redirects WHERE old_owner_slug=$1 AND old_name=$2")
                .bind(&slug)
                .bind(&name)
                .execute(p)
                .await
                .map_err(|e| format!("delete redirect: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query("DELETE FROM repository_redirects WHERE old_owner_slug=? AND old_name=?")
                .bind(&slug)
                .bind(&name)
                .execute(p)
                .await
                .map_err(|e| format!("delete redirect: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query("DELETE FROM repository_redirects WHERE old_owner_slug=? AND old_name=?")
                .bind(&slug)
                .bind(&name)
                .execute(p)
                .await
                .map_err(|e| format!("delete redirect: {e}"))?;
        }
    }
    Ok(())
}

pub async fn purge_expired_redirects(pool: &DbPool, now_rfc3339: &str) -> Result<u64, String> {
    match pool {
        DbPool::Postgres(p) => {
            let res =
                sqlx::query("DELETE FROM repository_redirects WHERE expires_at < $1::timestamptz")
                    .bind(now_rfc3339)
                    .execute(p)
                    .await
                    .map_err(|e| format!("purge: {e}"))?;
            Ok(res.rows_affected())
        }
        DbPool::MySql(p) => {
            let res = sqlx::query("DELETE FROM repository_redirects WHERE expires_at < STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')").bind(now_rfc3339).execute(p).await.map_err(|e| format!("purge: {e}"))?;
            Ok(res.rows_affected())
        }
        DbPool::Sqlite(p) => {
            let stored = now_rfc3339.trim_end_matches('Z').replace('T', " ");
            let res = sqlx::query("DELETE FROM repository_redirects WHERE expires_at < ?")
                .bind(&stored)
                .execute(p)
                .await
                .map_err(|e| format!("purge: {e}"))?;
            Ok(res.rows_affected())
        }
    }
}
