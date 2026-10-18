//! `actions/cache` service storage (0062) — committed cache entries plus the
//! per-job bearer tokens that scope `_apis/artifactcache` access to one job.

use crate::DbPool;

/// One cache entry row — `reserved` until the upload commits.
#[derive(Debug, Clone)]
pub struct ActionCacheRow {
    pub id: String,
    pub repository_id: String,
    /// sha256(key \n version \n scope), computed by the caller — the DB only
    /// stores and compares the opaque hex string.
    pub lookup_hash: String,
    pub cache_key: String,
    pub version: String,
    /// Branch scope ("" = unscoped / legacy).
    pub scope: String,
    pub storage_path: String,
    pub size_bytes: i64,
    /// `reserved` | `committed`.
    pub state: String,
    pub created_at: String,
    pub last_accessed_at: String,
}

/// Per-job cache-service token — stored hashed like runner tokens.
#[derive(Debug, Clone)]
pub struct ActionCacheTokenRow {
    pub job_id: String,
    pub repository_id: String,
    pub expires_at: String,
}

const CACHE_COLS: &str = "id, repository_id, lookup_hash, cache_key, version, scope, storage_path, size_bytes, state, created_at, last_accessed_at";
/// MySQL DATETIME columns do not decode into String — project RFC3339 text.
const CACHE_COLS_MYSQL: &str = "id, repository_id, lookup_hash, cache_key, version, scope, storage_path, size_bytes, state, DATE_FORMAT(created_at, '%Y-%m-%dT%H:%i:%sZ') AS created_at, DATE_FORMAT(last_accessed_at, '%Y-%m-%dT%H:%i:%sZ') AS last_accessed_at";
/// SQLite stores `%Y-%m-%d %H:%M:%S` — reproject RFC3339 like siblings do.
const CACHE_COLS_SQLITE: &str = "id, repository_id, lookup_hash, cache_key, version, scope, storage_path, size_bytes, state, strftime('%Y-%m-%dT%H:%M:%SZ', created_at) AS created_at, strftime('%Y-%m-%dT%H:%M:%SZ', last_accessed_at) AS last_accessed_at";

macro_rules! map_cache_row {
    ($row:expr) => {{
        use sqlx::Row;
        let r = $row;
        ActionCacheRow {
            id: r.get("id"),
            repository_id: r.get("repository_id"),
            lookup_hash: r.get("lookup_hash"),
            cache_key: r.get("cache_key"),
            version: r.get("version"),
            scope: r.get("scope"),
            storage_path: r.get("storage_path"),
            size_bytes: r
                .try_get::<i64, _>("size_bytes")
                .or_else(|_| r.try_get::<i32, _>("size_bytes").map(i64::from))
                .unwrap_or(0),
            state: r.get("state"),
            created_at: r.try_get("created_at").unwrap_or_default(),
            last_accessed_at: r.try_get("last_accessed_at").unwrap_or_default(),
        }
    }};
}

/// Exact-match lookup by (repo, key, version, scope) — the `cache?keys&version`
/// primary-key probe. Only `committed` entries count (a reserved entry holds
/// the key so a racing save 409s, but is not downloadable).
pub async fn find_action_cache(
    pool: &DbPool,
    repo_id: &str,
    lookup_hash: &str,
) -> Result<Option<ActionCacheRow>, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_SQLITE} FROM action_caches WHERE repository_id = ? AND lookup_hash = ? AND state = 'committed'"
        )))
        .bind(repo_id)
        .bind(lookup_hash)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_cache_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS} FROM action_caches WHERE repository_id = $1 AND lookup_hash = $2 AND state = 'committed'"
        )))
        .bind(repo_id)
        .bind(lookup_hash)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_cache_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_MYSQL} FROM action_caches WHERE repository_id = ? AND lookup_hash = ? AND state = 'committed'"
        )))
        .bind(repo_id)
        .bind(lookup_hash)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_cache_row!(r)))
        .map_err(|e| e.to_string()),
    }
}

/// By-id probe — any state (reserved or committed); the caller filters.
pub async fn find_action_cache_by_id(
    pool: &DbPool,
    id: &str,
) -> Result<Option<ActionCacheRow>, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_SQLITE} FROM action_caches WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_cache_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS} FROM action_caches WHERE id = $1"
        )))
        .bind(id)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_cache_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_MYSQL} FROM action_caches WHERE id = ?"
        )))
        .bind(id)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_cache_row!(r)))
        .map_err(|e| e.to_string()),
    }
}

/// Prefix fallback for `restore-keys` — committed rows whose `cache_key`
/// starts with `prefix`, newest first. The caller filters by version/scope
/// and picks the head of the list.
pub async fn find_action_caches_by_key_prefix(
    pool: &DbPool,
    repo_id: &str,
    prefix: &str,
    scope: &str,
) -> Result<Vec<ActionCacheRow>, String> {
    // Escape LIKE metachars — a hostile prefix must not widen the match.
    let pat = format!(
        "{}%",
        prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_SQLITE} FROM action_caches WHERE repository_id = ? AND cache_key LIKE ? ESCAPE '\\' AND scope = ? AND state = 'committed' ORDER BY created_at DESC"
        )))
        .bind(repo_id)
        .bind(&pat)
        .bind(scope)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_cache_row!(r)).collect())
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS} FROM action_caches WHERE repository_id = $1 AND cache_key LIKE $2 ESCAPE '\\' AND scope = $3 AND state = 'committed' ORDER BY created_at DESC"
        )))
        .bind(repo_id)
        .bind(&pat)
        .bind(scope)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_cache_row!(r)).collect())
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_MYSQL} FROM action_caches WHERE repository_id = ? AND cache_key LIKE ? ESCAPE '\\\\' AND scope = ? AND state = 'committed' ORDER BY created_at DESC"
        )))
        .bind(repo_id)
        .bind(&pat)
        .bind(scope)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_cache_row!(r)).collect())
        .map_err(|e| e.to_string()),
    }
}

/// Reserve a cache entry (`POST /caches`). Returns `Err("conflict")` when the
/// (repo, lookup_hash) pair already exists — GitHub immutable-save semantics:
/// a present key makes the save a no-op (actions/cache reports
/// `Unable to reserve cache` → `cache-hit` stays unset).
pub async fn insert_action_cache(pool: &DbPool, c: &ActionCacheRow) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "INSERT INTO action_caches (id, repository_id, lookup_hash, cache_key, version, scope, storage_path, size_bytes, state) \
             VALUES (?,?,?,?,?,?,?,?,'reserved')",
        )
        .bind(&c.id)
        .bind(&c.repository_id)
        .bind(&c.lookup_hash)
        .bind(&c.cache_key)
        .bind(&c.version)
        .bind(&c.scope)
        .bind(&c.storage_path)
        .bind(c.size_bytes)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO action_caches (id, repository_id, lookup_hash, cache_key, version, scope, storage_path, size_bytes, state) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'reserved')",
        )
        .bind(&c.id)
        .bind(&c.repository_id)
        .bind(&c.lookup_hash)
        .bind(&c.cache_key)
        .bind(&c.version)
        .bind(&c.scope)
        .bind(&c.storage_path)
        .bind(c.size_bytes)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
        DbPool::MySql(p) => sqlx::query(
            "INSERT INTO action_caches (id, repository_id, lookup_hash, cache_key, version, scope, storage_path, size_bytes, state) \
             VALUES (?,?,?,?,?,?,?,?,'reserved')",
        )
        .bind(&c.id)
        .bind(&c.repository_id)
        .bind(&c.lookup_hash)
        .bind(&c.cache_key)
        .bind(&c.version)
        .bind(&c.scope)
        .bind(&c.storage_path)
        .bind(c.size_bytes)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
    };
    Ok(())
}

/// Finalize a reserved entry (`POST /caches/{id}`): stamps the real size and
/// flips `reserved` → `committed`. Returns false when the id isn't a reserved
/// row (double-commit / unknown id).
pub async fn commit_action_cache(pool: &DbPool, id: &str, size_bytes: i64) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Sqlite(p) => {
            sqlx::query("UPDATE action_caches SET state = 'committed', size_bytes = ?, last_accessed_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ? AND state = 'reserved'")
                .bind(size_bytes)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected()
        }
        DbPool::Postgres(p) => {
            sqlx::query("UPDATE action_caches SET state = 'committed', size_bytes = $1, last_accessed_at = now() WHERE id = $2 AND state = 'reserved'")
                .bind(size_bytes)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected()
        }
        DbPool::MySql(p) => {
            sqlx::query("UPDATE action_caches SET state = 'committed', size_bytes = ?, last_accessed_at = NOW() WHERE id = ? AND state = 'reserved'")
                .bind(size_bytes)
                .bind(id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected()
        }
    };
    Ok(affected > 0)
}

/// Bump `last_accessed_at` on a download (LRU basis for eviction).
pub async fn touch_action_cache(pool: &DbPool, id: &str) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "UPDATE action_caches SET last_accessed_at = strftime('%Y-%m-%d %H:%M:%S','now') WHERE id = ?",
        )
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
        DbPool::Postgres(p) => sqlx::query(
            "UPDATE action_caches SET last_accessed_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
        DbPool::MySql(p) => sqlx::query(
            "UPDATE action_caches SET last_accessed_at = NOW() WHERE id = ?",
        )
        .bind(id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
    };
    Ok(())
}

/// Delete a cache row by id (caller owns the payload file). Returns whether
/// a row existed — used by eviction and the management DELETE endpoint.
pub async fn delete_action_cache(pool: &DbPool, id: &str) -> Result<bool, String> {
    let affected = match pool {
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM action_caches WHERE id = ?")
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| e.to_string())?,
        DbPool::Postgres(p) => sqlx::query("DELETE FROM action_caches WHERE id = $1")
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| e.to_string())?,
        DbPool::MySql(p) => sqlx::query("DELETE FROM action_caches WHERE id = ?")
            .bind(id)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| e.to_string())?,
    };
    Ok(affected > 0)
}

/// Repo cache usage + oldest-first listing for eviction — committed rows
/// ordered by `last_accessed_at` ASC with `id` as tie-break (evict from the
/// head until under cap).
pub async fn list_action_caches_for_repo(
    pool: &DbPool,
    repo_id: &str,
) -> Result<Vec<ActionCacheRow>, String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_SQLITE} FROM action_caches WHERE repository_id = ? AND state = 'committed' ORDER BY last_accessed_at ASC, id ASC"
        )))
        .bind(repo_id)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_cache_row!(r)).collect())
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS} FROM action_caches WHERE repository_id = $1 AND state = 'committed' ORDER BY last_accessed_at ASC, id ASC"
        )))
        .bind(repo_id)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_cache_row!(r)).collect())
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(sqlx::AssertSqlSafe(format!(
            "SELECT {CACHE_COLS_MYSQL} FROM action_caches WHERE repository_id = ? AND state = 'committed' ORDER BY last_accessed_at ASC, id ASC"
        )))
        .bind(repo_id)
        .fetch_all(p)
        .await
        .map(|rs| rs.into_iter().map(|r| map_cache_row!(r)).collect())
        .map_err(|e| e.to_string()),
    }
}

/// Reap `reserved` rows that never committed (abandoned uploads). `cutoff`
/// is RFC3339 — rows created before it are deleted and their (likely
/// nonexistent) payload paths returned for cleanup.
pub async fn reap_stale_action_cache_reservations(
    pool: &DbPool,
    cutoff: &str,
) -> Result<Vec<String>, String> {
    let paths: Vec<String> = match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "DELETE FROM action_caches WHERE state = 'reserved' AND created_at < ? RETURNING storage_path",
        )
        .bind(cutoff)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|r| {
            use sqlx::Row;
            r.try_get::<String, _>("storage_path").ok()
        })
        .collect(),
        DbPool::Postgres(p) => sqlx::query(
            "DELETE FROM action_caches WHERE state = 'reserved' AND created_at < $1::timestamptz RETURNING storage_path",
        )
        .bind(cutoff)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .filter_map(|r| {
            use sqlx::Row;
            r.try_get::<String, _>("storage_path").ok()
        })
        .collect(),
        DbPool::MySql(p) => {
            // MySQL has no RETURNING — select paths, then delete.
            let paths: Vec<String> = sqlx::query(
                "SELECT storage_path FROM action_caches WHERE state = 'reserved' AND created_at < STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')",
            )
            .bind(cutoff)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?
            .iter()
            .filter_map(|r| {
                use sqlx::Row;
                r.try_get::<String, _>("storage_path").ok()
            })
            .collect();
            sqlx::query(
                "DELETE FROM action_caches WHERE state = 'reserved' AND created_at < STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')",
            )
            .bind(cutoff)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            paths
        }
    };
    Ok(paths)
}

// --- per-job cache tokens ---------------------------------------------------

/// Mint a cache-service token row (hash persisted; plaintext lives with the
/// claiming runner). `expires_at` RFC3339.
pub async fn insert_action_cache_token(
    pool: &DbPool,
    token_hash: &str,
    job_id: &str,
    repository_id: &str,
    expires_at: &str,
) -> Result<(), String> {
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "INSERT INTO action_cache_tokens (token_hash, job_id, repository_id, expires_at) VALUES (?,?,?,?)",
        )
        .bind(token_hash)
        .bind(job_id)
        .bind(repository_id)
        .bind(expires_at)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
        DbPool::Postgres(p) => sqlx::query(
            "INSERT INTO action_cache_tokens (token_hash, job_id, repository_id, expires_at) VALUES ($1,$2,$3,$4::timestamptz)",
        )
        .bind(token_hash)
        .bind(job_id)
        .bind(repository_id)
        .bind(expires_at)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
        DbPool::MySql(p) => sqlx::query(
            "INSERT INTO action_cache_tokens (token_hash, job_id, repository_id, expires_at) VALUES (?,?,?,STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s'))",
        )
        .bind(token_hash)
        .bind(job_id)
        .bind(repository_id)
        .bind(expires_at)
        .execute(p)
        .await
        .map_err(|e| e.to_string())
        .map(|_| ())?,
    };
    Ok(())
}

/// Resolve a presented bearer token → job + repo scope. Only unexpired rows
/// resolve — `now` is RFC3339 (format-stable text comparison).
pub async fn find_action_cache_token(
    pool: &DbPool,
    token_hash: &str,
    now: &str,
) -> Result<Option<ActionCacheTokenRow>, String> {
    macro_rules! map_tok_row {
        ($r:expr) => {{
            use sqlx::Row;
            ActionCacheTokenRow {
                job_id: $r.get("job_id"),
                repository_id: $r.get("repository_id"),
                expires_at: $r.try_get("expires_at").unwrap_or_default(),
            }
        }};
    }
    match pool {
        DbPool::Sqlite(p) => sqlx::query(
            "SELECT job_id, repository_id, expires_at FROM action_cache_tokens WHERE token_hash = ? AND expires_at > ?",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_tok_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::Postgres(p) => sqlx::query(
            "SELECT job_id, repository_id, expires_at::text AS expires_at FROM action_cache_tokens WHERE token_hash = $1 AND expires_at > $2::timestamptz",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_tok_row!(r)))
        .map_err(|e| e.to_string()),
        DbPool::MySql(p) => sqlx::query(
            "SELECT job_id, repository_id, DATE_FORMAT(expires_at, '%Y-%m-%dT%H:%i:%sZ') AS expires_at FROM action_cache_tokens WHERE token_hash = ? AND expires_at > STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')",
        )
        .bind(token_hash)
        .bind(now)
        .fetch_optional(p)
        .await
        .map(|r| r.map(|r| map_tok_row!(r)))
        .map_err(|e| e.to_string()),
    }
}

/// Drop expired token rows — swept on a timer; `now` RFC3339.
pub async fn reap_action_cache_tokens(pool: &DbPool, now: &str) -> Result<u64, String> {
    let affected = match pool {
        DbPool::Sqlite(p) => sqlx::query("DELETE FROM action_cache_tokens WHERE expires_at <= ?")
            .bind(now)
            .execute(p)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| e.to_string())?,
        DbPool::Postgres(p) => {
            sqlx::query("DELETE FROM action_cache_tokens WHERE expires_at <= $1::timestamptz")
                .bind(now)
                .execute(p)
                .await
                .map(|r| r.rows_affected())
                .map_err(|e| e.to_string())?
        }
        DbPool::MySql(p) => sqlx::query(
            "DELETE FROM action_cache_tokens WHERE expires_at <= STR_TO_DATE(REPLACE(REPLACE(?, 'T', ' '), 'Z', ''), '%Y-%m-%d %H:%i:%s')",
        )
        .bind(now)
        .execute(p)
        .await
        .map(|r| r.rows_affected())
        .map_err(|e| e.to_string())?,
    };
    Ok(affected)
}
