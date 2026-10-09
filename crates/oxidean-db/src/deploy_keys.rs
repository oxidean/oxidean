//! Per-repo SSH deploy key CRUD via `DbPool` match (GIT-23).
//!
//! Deploy keys are transport-only credentials: they authenticate Git-over-SSH
//! pack operations for exactly one repository and never resolve to an account
//! identity (no session, no RPC, no web access). The same public key may be
//! attached to multiple repositories, but only once per repo —
//! `UNIQUE(repo_id, fingerprint)`. Delete is a hard-delete; repo deletion
//! cascades.

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct DeployKeyRow {
    pub id: String,
    pub repo_id: String,
    pub title: String,
    pub public_key: String,
    pub fingerprint: String,
    pub key_type: String,
    pub can_write: bool,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

macro_rules! map_deploy_key {
    ($row:expr) => {{
        let row = $row;
        let can_write = row
            .try_get::<i64, _>("can_write")
            .or_else(|_| row.try_get::<i8, _>("can_write").map(|v| i64::from(v)))
            .or_else(|_| {
                row.try_get::<bool, _>("can_write")
                    .map(|v| if v { 1 } else { 0 })
            })
            .unwrap_or(0)
            != 0;
        DeployKeyRow {
            id: row
                .try_get("id")
                .map_err(|e| format!("deploy key row: {e}"))?,
            repo_id: row
                .try_get("repo_id")
                .map_err(|e| format!("deploy key row: {e}"))?,
            title: row
                .try_get("title")
                .map_err(|e| format!("deploy key row: {e}"))?,
            public_key: row
                .try_get("public_key")
                .map_err(|e| format!("deploy key row: {e}"))?,
            fingerprint: row
                .try_get("fingerprint")
                .map_err(|e| format!("deploy key row: {e}"))?,
            key_type: row
                .try_get("key_type")
                .map_err(|e| format!("deploy key row: {e}"))?,
            can_write,
            last_used_at: row
                .try_get("last_used_at")
                .map_err(|e| format!("deploy key row: {e}"))?,
            last_used_ip: row
                .try_get("last_used_ip")
                .map_err(|e| format!("deploy key row: {e}"))?,
            created_by: row
                .try_get("created_by")
                .map_err(|e| format!("deploy key row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("deploy key row: {e}"))?,
        }
    }};
}

const DEPLOY_KEY_SELECT_PG: &str = "SELECT id, repo_id, title, public_key, fingerprint, key_type,
       CASE WHEN can_write THEN 1 ELSE 0 END AS can_write,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE to_char(last_used_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS last_used_at,
       last_used_ip,
       created_by,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM deploy_keys";

const DEPLOY_KEY_SELECT_MYSQL: &str =
    "SELECT id, repo_id, title, public_key, fingerprint, key_type,
       can_write,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE DATE_FORMAT(last_used_at, '%Y-%m-%dT%H:%i:%sZ') END AS last_used_at,
       last_used_ip,
       created_by,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM deploy_keys";

const DEPLOY_KEY_SELECT_SQLITE: &str =
    "SELECT id, repo_id, title, public_key, fingerprint, key_type,
       can_write,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at) END AS last_used_at,
       last_used_ip,
       created_by,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM deploy_keys";

/// Insert a deploy key row (id, repo, title, key material, scope, creator).
#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &DbPool,
    id: &str,
    repo_id: &str,
    title: &str,
    public_key: &str,
    fingerprint: &str,
    key_type: &str,
    can_write: bool,
    created_by: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "INSERT INTO deploy_keys
(id, repo_id, title, public_key, fingerprint, key_type, can_write, created_by)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(title)
            .bind(public_key)
            .bind(fingerprint)
            .bind(key_type)
            .bind(can_write)
            .bind(created_by)
            .execute(p)
            .await
            .map_err(|e| format!("create deploy key failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "INSERT INTO deploy_keys
(id, repo_id, title, public_key, fingerprint, key_type, can_write, created_by)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(title)
            .bind(public_key)
            .bind(fingerprint)
            .bind(key_type)
            .bind(can_write)
            .bind(created_by)
            .execute(p)
            .await
            .map_err(|e| format!("create deploy key failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "INSERT INTO deploy_keys
(id, repo_id, title, public_key, fingerprint, key_type, can_write, created_by)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )
            .bind(id)
            .bind(repo_id)
            .bind(title)
            .bind(public_key)
            .bind(fingerprint)
            .bind(key_type)
            .bind(if can_write { 1 } else { 0 })
            .bind(created_by)
            .execute(p)
            .await
            .map_err(|e| format!("create deploy key failed: {e}"))?;
        }
    }
    Ok(())
}

/// Any deploy key row matching an OpenSSH SHA256 fingerprint (`SHA256:…`).
/// Used at SSH auth time before the target repo is known; repo scope is
/// re-checked per pack exec via [`find_for_repo`].
pub async fn find_by_fingerprint(
    pool: &DbPool,
    fingerprint: &str,
) -> Result<Option<DeployKeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_PG} WHERE fingerprint = $1 ORDER BY created_at ASC, id ASC LIMIT 1"
            )))
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find deploy key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deploy_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_MYSQL} WHERE fingerprint = ? ORDER BY created_at ASC, id ASC LIMIT 1"
            )))
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find deploy key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deploy_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_SQLITE} WHERE fingerprint = ?1 ORDER BY created_at ASC, id ASC LIMIT 1"
            )))
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find deploy key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deploy_key!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// The deploy key attached to `repo_id` with this fingerprint (scope check at
/// pack-exec time; the same key may serve several repos).
pub async fn find_for_repo(
    pool: &DbPool,
    repo_id: &str,
    fingerprint: &str,
) -> Result<Option<DeployKeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_PG} WHERE repo_id = $1 AND fingerprint = $2"
            )))
            .bind(repo_id)
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find deploy key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deploy_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_MYSQL} WHERE repo_id = ? AND fingerprint = ?"
            )))
            .bind(repo_id)
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find deploy key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deploy_key!(&r))),
                None => Ok(None),
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_SQLITE} WHERE repo_id = ?1 AND fingerprint = ?2"
            )))
            .bind(repo_id)
            .bind(fingerprint)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find deploy key failed: {e}"))?;
            match row {
                Some(r) => Ok(Some(map_deploy_key!(&r))),
                None => Ok(None),
            }
        }
    }
}

/// Keys attached to a repo, newest first (`created_at DESC`).
pub async fn list_for_repo(pool: &DbPool, repo_id: &str) -> Result<Vec<DeployKeyRow>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_PG} WHERE repo_id = $1 ORDER BY created_at DESC, id DESC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list deploy keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_deploy_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_MYSQL} WHERE repo_id = ? ORDER BY created_at DESC, id DESC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list deploy keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_deploy_key!(&r));
            }
            Ok(mapped)
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{DEPLOY_KEY_SELECT_SQLITE} WHERE repo_id = ?1 ORDER BY created_at DESC, id DESC"
            )))
            .bind(repo_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list deploy keys failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_deploy_key!(&r));
            }
            Ok(mapped)
        }
    }
}

/// Hard-delete a key scoped to its repo. Returns true when a row was removed.
pub async fn revoke(pool: &DbPool, repo_id: &str, id: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM deploy_keys WHERE id = $1 AND repo_id = $2")
                .bind(id)
                .bind(repo_id)
                .execute(p)
                .await
                .map_err(|e| format!("revoke deploy key failed: {e}"))?
                .rows_affected()
        }
        DbPool::MySql(p) => sqlx::query("DELETE FROM deploy_keys WHERE id = ? AND repo_id = ?")
            .bind(id)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| format!("revoke deploy key failed: {e}"))?
            .rows_affected(),
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM deploy_keys WHERE id = ?1 AND repo_id = ?2")
            .bind(id)
            .bind(repo_id)
            .execute(p)
            .await
            .map_err(|e| format!("revoke deploy key failed: {e}"))?
            .rows_affected(),
    };
    Ok(affected > 0)
}

/// Update last-used timestamp and optional client IP after a successful
/// repo-scoped pack authorization (exec time — auth alone is not repo-bound).
pub async fn touch_last_used(
    pool: &DbPool,
    id: &str,
    last_used_at: &str,
    last_used_ip: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE deploy_keys
SET last_used_at = $2::timestamptz, last_used_ip = $3
WHERE id = $1",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch deploy key failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE deploy_keys SET last_used_at = ?, last_used_ip = ?
WHERE id = ?",
            )
            .bind(last_used_at)
            .bind(last_used_ip)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("touch deploy key failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE deploy_keys SET last_used_at = ?2, last_used_ip = ?3
WHERE id = ?1",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch deploy key failed: {e}"))?;
        }
    }
    Ok(())
}
