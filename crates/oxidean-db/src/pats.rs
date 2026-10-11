//! Personal access token CRUD via `DbPool` match — hash-at-rest only (D-15, T-08-01).

use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, Clone)]
pub struct PatRow {
    pub id: String,
    pub user_id: String,
    pub kind: String,
    pub name: String,
    pub token_prefix: String,
    pub token_hash: String,
    pub scopes_json: Option<String>,
    pub contents_perm: Option<String>,
    pub repo_access: Option<String>,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub last_used_ip: Option<String>,
    pub created_at: String,
    /// Loaded from `personal_access_token_repos` for fine-grained selected repos.
    pub repository_ids: Vec<String>,
}

macro_rules! map_pat {
    ($row:expr) => {{
        let row = $row;
        PatRow {
            id: row.try_get("id").map_err(|e| format!("pat row: {e}"))?,
            user_id: row
                .try_get("user_id")
                .map_err(|e| format!("pat row: {e}"))?,
            kind: row.try_get("kind").map_err(|e| format!("pat row: {e}"))?,
            name: row.try_get("name").map_err(|e| format!("pat row: {e}"))?,
            token_prefix: row
                .try_get("token_prefix")
                .map_err(|e| format!("pat row: {e}"))?,
            token_hash: row
                .try_get("token_hash")
                .map_err(|e| format!("pat row: {e}"))?,
            scopes_json: row
                .try_get("scopes_json")
                .map_err(|e| format!("pat row: {e}"))?,
            contents_perm: row
                .try_get("contents_perm")
                .map_err(|e| format!("pat row: {e}"))?,
            repo_access: row
                .try_get("repo_access")
                .map_err(|e| format!("pat row: {e}"))?,
            expires_at: row
                .try_get("expires_at")
                .map_err(|e| format!("pat row: {e}"))?,
            revoked_at: row
                .try_get("revoked_at")
                .map_err(|e| format!("pat row: {e}"))?,
            last_used_at: row
                .try_get("last_used_at")
                .map_err(|e| format!("pat row: {e}"))?,
            last_used_ip: row
                .try_get("last_used_ip")
                .map_err(|e| format!("pat row: {e}"))?,
            created_at: row
                .try_get("created_at")
                .map_err(|e| format!("pat row: {e}"))?,
            repository_ids: Vec::new(),
        }
    }};
}

const PAT_SELECT_PG: &str = "SELECT id, user_id, kind, name, token_prefix, token_hash, scopes_json,
       contents_perm, repo_access,
       CASE WHEN expires_at IS NULL THEN NULL
            ELSE to_char(expires_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS expires_at,
       CASE WHEN revoked_at IS NULL THEN NULL
            ELSE to_char(revoked_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS revoked_at,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE to_char(last_used_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') END AS last_used_at,
       last_used_ip,
       to_char(created_at AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"') AS created_at
FROM personal_access_tokens";

const PAT_SELECT_MYSQL: &str =
    "SELECT id, user_id, kind, name, token_prefix, token_hash, scopes_json,
       contents_perm, repo_access,
       CASE WHEN expires_at IS NULL THEN NULL
            ELSE DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') END AS expires_at,
       CASE WHEN revoked_at IS NULL THEN NULL
            ELSE DATE_FORMAT(revoked_at, '%Y-%m-%dT%H:%i:%sZ') END AS revoked_at,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE DATE_FORMAT(last_used_at, '%Y-%m-%dT%H:%i:%sZ') END AS last_used_at,
       last_used_ip,
       DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at
FROM personal_access_tokens";

const PAT_SELECT_SQLITE: &str =
    "SELECT id, user_id, kind, name, token_prefix, token_hash, scopes_json,
       contents_perm, repo_access,
       CASE WHEN expires_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', expires_at) END AS expires_at,
       CASE WHEN revoked_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', revoked_at) END AS revoked_at,
       CASE WHEN last_used_at IS NULL THEN NULL
            ELSE strftime('%Y-%m-%dT%H:%M:%SZ', last_used_at) END AS last_used_at,
       last_used_ip,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at
FROM personal_access_tokens";

async fn load_repo_ids(pool: &DbPool, token_id: &str) -> Result<Vec<String>, String> {
    match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(
                "SELECT repository_id FROM personal_access_token_repos WHERE token_id = $1 ORDER BY repository_id",
            )
            .bind(token_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pat repos failed: {e}"))?;
            rows.into_iter()
                .map(|r| {
                    r.try_get("repository_id")
                        .map_err(|e| format!("pat repo row: {e}"))
                })
                .collect()
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(
                "SELECT repository_id FROM personal_access_token_repos WHERE token_id = ? ORDER BY repository_id",
            )
            .bind(token_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pat repos failed: {e}"))?;
            rows.into_iter()
                .map(|r| {
                    r.try_get("repository_id")
                        .map_err(|e| format!("pat repo row: {e}"))
                })
                .collect()
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(
                "SELECT repository_id FROM personal_access_token_repos WHERE token_id = ?1 ORDER BY repository_id",
            )
            .bind(token_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pat repos failed: {e}"))?;
            rows.into_iter()
                .map(|r| {
                    r.try_get("repository_id")
                        .map_err(|e| format!("pat repo row: {e}"))
                })
                .collect()
        }
    }
}

/// Insert a PAT row and optional FG selected-repo links in one transaction.
/// Roll back on any link failure so create never leaves an orphan PAT without plaintext (WR-01).
#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &DbPool,
    id: &str,
    user_id: &str,
    kind: &str,
    name: &str,
    token_prefix: &str,
    token_hash: &str,
    scopes_json: Option<&str>,
    contents_perm: Option<&str>,
    repo_access: Option<&str>,
    expires_at: Option<&str>,
    repository_ids: &[String],
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin pat create tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO personal_access_tokens
(id, user_id, kind, name, token_prefix, token_hash, scopes_json, contents_perm, repo_access, expires_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::timestamptz)",
            )
            .bind(id)
            .bind(user_id)
            .bind(kind)
            .bind(name)
            .bind(token_prefix)
            .bind(token_hash)
            .bind(scopes_json)
            .bind(contents_perm)
            .bind(repo_access)
            .bind(expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("create pat failed: {e}"))?;
            for repo_id in repository_ids {
                sqlx::query(
                    "INSERT INTO personal_access_token_repos (token_id, repository_id) VALUES ($1, $2)",
                )
                .bind(id)
                .bind(repo_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert pat repo link failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit pat create tx failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin pat create tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO personal_access_tokens
(id, user_id, kind, name, token_prefix, token_hash, scopes_json, contents_perm, repo_access, expires_at)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(kind)
            .bind(name)
            .bind(token_prefix)
            .bind(token_hash)
            .bind(scopes_json)
            .bind(contents_perm)
            .bind(repo_access)
            .bind(expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("create pat failed: {e}"))?;
            for repo_id in repository_ids {
                sqlx::query(
                    "INSERT INTO personal_access_token_repos (token_id, repository_id) VALUES (?, ?)",
                )
                .bind(id)
                .bind(repo_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert pat repo link failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit pat create tx failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| format!("begin pat create tx failed: {e}"))?;
            sqlx::query(
                "INSERT INTO personal_access_tokens
(id, user_id, kind, name, token_prefix, token_hash, scopes_json, contents_perm, repo_access, expires_at)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .bind(id)
            .bind(user_id)
            .bind(kind)
            .bind(name)
            .bind(token_prefix)
            .bind(token_hash)
            .bind(scopes_json)
            .bind(contents_perm)
            .bind(repo_access)
            .bind(expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| format!("create pat failed: {e}"))?;
            for repo_id in repository_ids {
                sqlx::query(
                    "INSERT INTO personal_access_token_repos (token_id, repository_id) VALUES (?1, ?2)",
                )
                .bind(id)
                .bind(repo_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert pat repo link failed: {e}"))?;
            }
            tx.commit()
                .await
                .map_err(|e| format!("commit pat create tx failed: {e}"))?;
        }
    }
    Ok(())
}

/// Lookup by SHA-256 hex. Returns `None` when missing or soft-revoked.
pub async fn find_by_token_hash(pool: &DbPool, token_hash: &str) -> Result<Option<PatRow>, String> {
    let mut pat = match pool {
        DbPool::Postgres(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PAT_SELECT_PG} WHERE token_hash = $1 AND revoked_at IS NULL"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find pat failed: {e}"))?;
            match row {
                Some(r) => Some(map_pat!(&r)),
                None => None,
            }
        }
        DbPool::MySql(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PAT_SELECT_MYSQL} WHERE token_hash = ? AND revoked_at IS NULL"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find pat failed: {e}"))?;
            match row {
                Some(r) => Some(map_pat!(&r)),
                None => None,
            }
        }
        DbPool::Sqlite(p) => {
            let row = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PAT_SELECT_SQLITE} WHERE token_hash = ?1 AND revoked_at IS NULL"
            )))
            .bind(token_hash)
            .fetch_optional(p)
            .await
            .map_err(|e| format!("find pat failed: {e}"))?;
            match row {
                Some(r) => Some(map_pat!(&r)),
                None => None,
            }
        }
    };
    if let Some(ref mut p) = pat {
        p.repository_ids = load_repo_ids(pool, &p.id).await?;
    }
    Ok(pat)
}

/// Active (non-revoked) PATs for a user, newest first.
pub async fn list_for_user(pool: &DbPool, user_id: &str) -> Result<Vec<PatRow>, String> {
    let mut out = match pool {
        DbPool::Postgres(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PAT_SELECT_PG} WHERE user_id = $1 AND revoked_at IS NULL
ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pats failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_pat!(&r));
            }
            mapped
        }
        DbPool::MySql(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PAT_SELECT_MYSQL} WHERE user_id = ? AND revoked_at IS NULL
ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pats failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_pat!(&r));
            }
            mapped
        }
        DbPool::Sqlite(p) => {
            let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
                "{PAT_SELECT_SQLITE} WHERE user_id = ?1 AND revoked_at IS NULL
ORDER BY created_at DESC, id DESC"
            )))
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| format!("list pats failed: {e}"))?;
            let mut mapped = Vec::with_capacity(rows.len());
            for r in rows {
                mapped.push(map_pat!(&r));
            }
            mapped
        }
    };
    for pat in &mut out {
        pat.repository_ids = load_repo_ids(pool, &pat.id).await?;
    }
    Ok(out)
}

/// Soft-revoke: set `revoked_at`. Idempotent if already revoked.
pub async fn revoke(pool: &DbPool, id: &str, revoked_at: &str) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE personal_access_tokens SET revoked_at = $2::timestamptz
WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(id)
            .bind(revoked_at)
            .execute(p)
            .await
            .map_err(|e| format!("revoke pat failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE personal_access_tokens SET revoked_at = ? WHERE id = ? AND revoked_at IS NULL",
            )
            .bind(revoked_at)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("revoke pat failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE personal_access_tokens SET revoked_at = ?2 WHERE id = ?1 AND revoked_at IS NULL",
            )
            .bind(id)
            .bind(revoked_at)
            .execute(p)
            .await
            .map_err(|e| format!("revoke pat failed: {e}"))?;
        }
    }
    Ok(())
}

/// Update last-used timestamp and optional client IP after successful Smart HTTP auth.
pub async fn touch_last_used(
    pool: &DbPool,
    id: &str,
    last_used_at: &str,
    last_used_ip: Option<&str>,
) -> Result<(), String> {
    match pool {
        DbPool::Postgres(p) => {
            sqlx::query(
                "UPDATE personal_access_tokens
SET last_used_at = $2::timestamptz, last_used_ip = $3
WHERE id = $1 AND revoked_at IS NULL",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch pat failed: {e}"))?;
        }
        DbPool::MySql(p) => {
            sqlx::query(
                "UPDATE personal_access_tokens SET last_used_at = ?, last_used_ip = ?
WHERE id = ? AND revoked_at IS NULL",
            )
            .bind(last_used_at)
            .bind(last_used_ip)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| format!("touch pat failed: {e}"))?;
        }
        DbPool::Sqlite(p) => {
            sqlx::query(
                "UPDATE personal_access_tokens SET last_used_at = ?2, last_used_ip = ?3
WHERE id = ?1 AND revoked_at IS NULL",
            )
            .bind(id)
            .bind(last_used_at)
            .bind(last_used_ip)
            .execute(p)
            .await
            .map_err(|e| format!("touch pat failed: {e}"))?;
        }
    }
    Ok(())
}
